INSERT INTO traces (trace_id, service, timestamp, min_duration, max_duration) VALUES (?, ?, ?, ?, ?)
ON CONFLICT(trace_id, service) DO UPDATE SET
    timestamp = min(timestamp, excluded.timestamp),
    min_duration = min(min_duration, excluded.min_duration),
    max_duration = max(max_duration, excluded.max_duration);
