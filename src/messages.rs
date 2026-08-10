use crate::db::{ItemId, Money};
use quinn::{RecvStream, SendStream};
use tracing::warn;
use wincode::error::{ReadResult, WriteResult};
use wincode::{SchemaRead, SchemaWrite};

pub const MAX_MESSAGE_SIZE: u32 = 2048;

#[derive(SchemaRead, SchemaWrite, Debug)]
pub enum ClientMessage {
    Ping,

    Register(String, String),
    Identity(String, String),

    Work,
    GetBalance,
    GetMyItems,
    GetAvailableItems,
    BuyItem(ItemId),
    SellItem(ItemId),
}

#[derive(SchemaRead, SchemaWrite, Debug)]
pub enum ServerResponse {
    Pong,

    Registered(String),
    RegistrationError(String),
    Authorized(String),
    AuthorizationError(String),

    Worked(Money),
    Balance(Money),
    BadWork(String),
    YouAreTooPoor,
    BadItem,
    ItemBought(ItemId),
    ItemSold(ItemId),
    ItemList(Vec<(ItemId, String, Money)>),
    YourItems(Vec<(String, String)>),

    Unauthorized,
    Error(String),
}

impl ClientMessage {
    pub fn serialize(&self) -> WriteResult<Vec<u8>> {
        wincode::serialize(self)
    }

    pub fn deserialize(data: &[u8]) -> ReadResult<Self> {
        wincode::deserialize(data)
    }
}

impl ServerResponse {
    pub fn serialize(&self) -> WriteResult<Vec<u8>> {
        wincode::serialize(self)
    }

    pub fn deserialize(data: &[u8]) -> ReadResult<Self> {
        wincode::deserialize(data)
    }
}

pub async fn send_frame(tx: &mut SendStream, data: &[u8]) -> anyhow::Result<()> {
    let size = data.len() as u32;
    let size_buf = size.to_le_bytes();
    tx.write(&size_buf).await?;
    tx.write(data).await?;
    Ok(())
}

pub async fn recv_frame(rx: &mut RecvStream) -> anyhow::Result<Vec<u8>> {
    let mut size_buf = [0u8; 4];
    rx.read_exact(&mut size_buf).await?;
    let size = u32::from_le_bytes(size_buf);
    if size > MAX_MESSAGE_SIZE {
        let _ = rx.stop(1u32.into());
        warn!("Too large payload");
        return Err(anyhow::anyhow!("Too large payload"));
    }
    let mut data_buf = vec![0u8; size as usize];
    rx.read_exact(&mut data_buf).await?;
    Ok(data_buf.to_vec())
}
