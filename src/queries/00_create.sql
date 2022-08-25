CREATE TABLE IF NOT EXISTS traces(
    trace_id     BLOB    NOT NULL,
    service      TEXT    NOT NULL,
    timestamp    TEXT    NOT NULL,
    min_duration INTEGER NOT NULL,
    max_duration INTEGER NOT NULL,
    PRIMARY KEY (trace_id, service)
) STRICT, WITHOUT ROWID;

CREATE TABLE IF NOT EXISTS spans(
    trace_id  BLOB NOT NULL,
    span_id   BLOB NOT NULL,
    operation TEXT NOT NULL,
    data      BLOB NOT NULL,
    PRIMARY KEY (trace_id, span_id)
) STRICT, WITHOUT ROWID;
