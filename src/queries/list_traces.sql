SELECT trace_id FROM traces
WHERE service = :service
    AND timestamp >= :t_min
    AND timestamp <= :t_max
    AND (:d_min IS NULL OR max_duration >= :d_min)
    AND (:d_max IS NULL OR min_duration <= :d_max)
ORDER BY timestamp DESC
LIMIT :limit;
