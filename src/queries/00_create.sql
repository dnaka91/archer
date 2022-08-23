CREATE TABLE IF NOT EXISTS spans(
    id        INTEGER PRIMARY KEY,
    trace_id  BLOB    NOT NULL,
    service   TEXT    NOT NULL,
    operation TEXT    NOT NULL,
    timestamp TEXT    NOT NULL,
    duration  INTEGER NOT NULL,
    data      BLOB    NOT NULL
) STRICT;
