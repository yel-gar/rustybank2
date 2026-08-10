#!/bin/sh
set -e

mkdir /app/data
sqlite3 $DB_FILE < ./res/db/init.sql && \
    sqlite3 $DB_FILE < ./res/db/populate.sql && \
    echo "UPDATE items SET description = '$FLAG' WHERE description = 'FLAG_PLACEHOLDER'" | sqlite3 $DB_FILE
