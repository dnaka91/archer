CREATE TABLE IF NOT EXISTS spans(
    id        INTEGER PRIMARY KEY,
    trace_id  BLOB    NOT NULL,
    service   TEXT    NOT NULL,
    operation TEXT    NOT NULL,
    data      BLOB    NOT NULL
) STRICT;
