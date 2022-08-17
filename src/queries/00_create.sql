CREATE TABLE IF NOT EXISTS spans(
    id INT PRIMARY KEY,
    trace_id BLOB NOT NULL,
    service STRING NOT NULL,
    operation STRING NOT NULL,
    data BLOB NOT NULL
);
