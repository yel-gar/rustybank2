use crate::cli::Args;
use crate::constants::{
    MAX_BI_STREAMS, MAX_IDLE_TIMEOUT, MONEY_PER_WORK, USERNAME_MAX_LENGTH, WORK_INTERVAL,
};
use crate::db::{Database, Money};
use crate::messages::{ClientMessage, ServerResponse, recv_frame, send_frame};
use crate::state::ConnectionState;
use crate::util::{hash_password, verify_password};
use anyhow::anyhow;
use quinn::rustls::pki_types::PrivateKeyDer;
use quinn::{Connection, Endpoint, RecvStream, SendStream, ServerConfig, TransportConfig, VarInt};
use rcgen::generate_simple_self_signed;
use std::collections::HashSet;
use std::net::{IpAddr, SocketAddr};
use std::sync::Arc;
use time::Timestamp;
use tokio::sync::RwLock;
use tracing::{Instrument, Span, debug, error, info, info_span, instrument, warn};

pub struct Server {
    endpoint: Endpoint,
    database: Arc<Database>,
    ip_limit_enabled: bool,
}

impl Server {
    pub async fn new(args: &Args) -> anyhow::Result<Self> {
        let database = Arc::new(Database::new(args).await?);
        let conf = make_conf()?;
        let bind_addr = SocketAddr::new(args.host, args.port);
        let endpoint = Endpoint::server(conf, bind_addr)?;

        info!("Server initialized");
        Ok(Self {
            endpoint,
            database,
            ip_limit_enabled: !args.disable_ip_limit,
        })
    }

    #[instrument(skip(self))]
    pub async fn run(self) {
        let addr_str = match self.endpoint.local_addr() {
            Ok(addr) => addr.to_string(),
            Err(_) => "(unknown)".to_string(),
        };
        info!("Server listening on {addr_str}");

        let connected_ips: Arc<RwLock<HashSet<IpAddr>>> = Default::default();

        while let Some(incoming) = self.endpoint.accept().await {
            info!("Incoming connection: {:?}", incoming.remote_address());
            if self.ip_limit_enabled
                && connected_ips
                    .read()
                    .await
                    .contains(&incoming.remote_address().ip())
            {
                warn!(
                    "Refusing connection from {} as this address already has an open connection",
                    incoming.remote_address()
                );
                incoming.refuse();
                continue;
            }

            match incoming.await {
                Ok(conn) => {
                    info!("Accepted connection from {}", conn.remote_address());
                    let conn_ip = conn.remote_address().ip();
                    let ips_ref = connected_ips.clone();
                    let db_ref = self.database.clone();
                    tokio::spawn(async move {
                        ips_ref.write().await.insert(conn_ip);

                        if let Err(e) = Self::handle_connection(conn, db_ref).await {
                            error!(%e, "Error occured during connection handling");
                        }

                        ips_ref.write().await.remove(&conn_ip);
                    });
                }
                Err(e) => {
                    error!(%e, "Connection failed");
                }
            }
        }
    }

    #[instrument(skip_all, fields(addr=conn.remote_address().to_string()))]
    async fn handle_connection(conn: Connection, db: Arc<Database>) -> anyhow::Result<()> {
        info!("Started connection handler");

        let state = Arc::new(ConnectionState::new(db));
        while let Ok((tx, rx)) = conn.accept_bi().await {
            info!("Incoming stream");
            let state_ref = state.clone();
            tokio::spawn(
                async move {
                    if let Err(e) = Self::handle_stream(tx, rx, state_ref).await {
                        error!(%e, "Error occurred during stream handling");
                    }
                }
                .instrument(Span::current()),
            );
        }
        info!("Connection handler closed");
        Ok(())
    }

    async fn handle_stream(
        mut tx: SendStream,
        mut rx: RecvStream,
        state: Arc<ConnectionState>,
    ) -> anyhow::Result<()> {
        info!("Handling stream");

        while let Ok(m) = recv_msg(&mut rx).await {
            debug!("Incoming message: {:?}", m);
            let resp = get_response(state.clone(), m).await.unwrap_or_else(|e| {
                error!(%e, "Unexpected error occured when generating response");
                ServerResponse::Error("Unknown error".to_string())
            });
            info!("Responding with {:?}", resp);
            if let Err(e) = send_msg(&mut tx, resp).await {
                warn!(%e, "Failed to respond to stream");
            }
        }
        info!("Done handling stream");
        Ok(())
    }
}

async fn get_response(
    state: Arc<ConnectionState>,
    message: ClientMessage,
) -> anyhow::Result<ServerResponse> {
    let user_id = state.user_id.read().await.clone();
    match (&user_id, message) {
        (_, ClientMessage::Ping) => {
            return Ok(ServerResponse::Pong);
        }

        (_, ClientMessage::GetAvailableItems) => {
            return Ok(ServerResponse::ItemList(
                state
                    .db
                    .get_all_items()
                    .await
                    .map_err(|_| anyhow!("Failed to get all items"))?,
            ));
        }

        (_, ClientMessage::Identity(username, password)) => {
            info!("Authorizing");
            let (hash_ok, user_id) = match state.db.get_user_hash_and_id(&username).await {
                Some((hash, id)) => (verify_password(password, hash), id),
                None => {
                    warn!("User doesn't exist");
                    return Ok(ServerResponse::AuthorizationError(
                        "Invalid password".to_string(),
                    ));
                }
            };
            if !hash_ok {
                warn!("Bad password");
                return Ok(ServerResponse::AuthorizationError(
                    "Invalid password".to_string(),
                ));
            }

            let _ = state.user_id.write().await.insert(user_id);
            info!("User authorized");
            return Ok(ServerResponse::Authorized(username));
        }

        (_, ClientMessage::Register(username, password)) => {
            info!("Registering");
            if username.len() > USERNAME_MAX_LENGTH {
                warn!("Too long username");
                return Ok(ServerResponse::AuthorizationError(
                    "Username too long (30 max)".to_string(),
                ));
            }
            let hash = hash_password(password).map_err(|_| anyhow!("Bad password"))?;
            return Ok(match state.db.insert_user(&username, hash).await {
                Ok(_) => {
                    info!("User registered successfully");
                    ServerResponse::Registered(username)
                }
                Err(e) => {
                    warn!(%e, "Username already taken");
                    ServerResponse::RegistrationError("Username already taken".to_string())
                }
            });
        }

        (None, _) => {
            return Ok(ServerResponse::Unauthorized);
        }

        (Some(id), m) => match m {
            ClientMessage::Work => {
                info!("Working");
                let now = Timestamp::now();
                let can_work_at = state
                    .db
                    .get_user_can_work_at(id)
                    .await
                    .ok_or(anyhow!("User doesn't exist"))?;
                let can_work_at =
                    Timestamp::from_seconds(can_work_at as i64).unwrap_or(Timestamp::now());
                if can_work_at > now {
                    warn!("User can't work yet");
                    let time_str =
                        format!("You can work again in {}", (can_work_at - now).to_string());
                    return Ok(ServerResponse::BadWork(time_str));
                }
                state
                    .db
                    .change_user_money(id, MONEY_PER_WORK as Money)
                    .await
                    .map_err(|_| anyhow!("Failed to change money to work"))?;
                state
                    .db
                    .set_user_can_work_at(id, (now + WORK_INTERVAL).as_seconds())
                    .await
                    .map_err(|_| anyhow!("Failed to update work time"))?;
                info!("Worked successfully");
                return Ok(ServerResponse::Worked(MONEY_PER_WORK));
            }
            ClientMessage::GetBalance => {
                return Ok(ServerResponse::Balance(
                    state
                        .db
                        .get_balance(id)
                        .await
                        .ok_or(anyhow!("Failed to get balance"))?,
                ));
            }
            ClientMessage::GetMyItems => {
                return Ok(ServerResponse::YourItems(
                    state
                        .db
                        .get_user_items(id)
                        .await
                        .map_err(|_| anyhow!("Failed to get user items"))?,
                ));
            }
            ClientMessage::BuyItem(item_id) => {
                info!("Buying item");
                let item_price = match state.db.get_item_price(&item_id).await {
                    Some(item_price) => item_price,
                    None => {
                        warn!("Attempting to buy non-existing item");
                        return Ok(ServerResponse::BadItem);
                    }
                };
                let user_money = state
                    .db
                    .get_balance(id)
                    .await
                    .ok_or(anyhow!("Failed to get balance"))?;
                if user_money < item_price {
                    warn!("User is too poor lmfao");
                    return Ok(ServerResponse::YouAreTooPoor);
                }
                state
                    .db
                    .add_user_item(id, &item_id)
                    .await
                    .map_err(|_| anyhow!("Failed to add item"))?;
                state
                    .db
                    .change_user_money(id, -(item_price as Money))
                    .await
                    .map_err(|_| anyhow!("Failed to take money"))?;
                return Ok(ServerResponse::ItemBought(item_id));
            }
            ClientMessage::SellItem(item_id) => {
                let has_item = state
                    .db
                    .get_user_has_item(id, &item_id)
                    .await
                    .map_err(|_| anyhow!("Failed to get user item ownership"))?;
                if !has_item {
                    warn!("User doesn't have this item or it does not exist");
                    return Ok(ServerResponse::BadItem);
                }
                let item_price = match state.db.get_item_price(&item_id).await {
                    Some(item_price) => item_price,
                    None => {
                        warn!("Attempting to sell non-existing item");
                        return Ok(ServerResponse::BadItem);
                    }
                };
                state
                    .db
                    .change_user_money(id, item_price as Money)
                    .await
                    .map_err(|_| anyhow!("Failed to give money"))?;
                state
                    .db
                    .remove_user_item(id, &item_id)
                    .await
                    .map_err(|_| anyhow!("Failed to remove item"))?;
                info!("Item sold successfully");
                return Ok(ServerResponse::ItemSold(item_id));
            }
            ClientMessage::Ping
            | ClientMessage::Register(_, _)
            | ClientMessage::GetAvailableItems
            | ClientMessage::Identity(_, _) => unreachable!(),
        },
    };
}

async fn recv_msg(rx: &mut RecvStream) -> anyhow::Result<ClientMessage> {
    ClientMessage::deserialize(recv_frame(rx).await?.as_slice())
        .map_err(|e| anyhow!("Failed to receive message: {e}"))
}

async fn send_msg(tx: &mut SendStream, msg: ServerResponse) -> anyhow::Result<()> {
    send_frame(tx, msg.serialize()?.as_slice())
        .await
        .map_err(|e| anyhow!("Failed to send message: {e}"))
}

fn make_conf() -> anyhow::Result<ServerConfig> {
    let cert = generate_simple_self_signed(vec!["localhost".to_string()])?;
    let cert_der = cert.cert.der().clone();
    let key_der = cert.signing_key.serialize_der();

    let mut conf =
        ServerConfig::with_single_cert(vec![cert_der], PrivateKeyDer::Pkcs8(key_der.into()))?;
    configure_transport(&mut conf);

    Ok(conf)
}

fn configure_transport(conf: &mut ServerConfig) {
    let mut transport = TransportConfig::default();
    transport.max_concurrent_bidi_streams(MAX_BI_STREAMS.into());
    transport.max_concurrent_uni_streams(0u32.into());
    transport.max_idle_timeout(Some(
        VarInt::from_u32(MAX_IDLE_TIMEOUT.as_millis() as u32).into(),
    ));
    conf.transport_config(Arc::new(transport));
}
