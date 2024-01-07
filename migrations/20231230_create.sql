CREATE TABLE IF NOT EXISTS frames_received
(
  id          INTEGER NOT NULL PRIMARY KEY,
  received_at INTEGER,
  content     BLOB
);
