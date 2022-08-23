SELECT trace_id, data FROM spans
WHERE service = :service
    AND (:op    IS NULL OR operation = :op)
    AND timestamp >= :t_min
    AND timestamp <= :t_max
    AND (:d_min IS NULL OR duration >= :d_min)
    AND (:d_max IS NULL OR duration <= :d_max)
ORDER BY trace_id;
