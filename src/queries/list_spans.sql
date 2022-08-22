SELECT trace_id, data FROM spans
WHERE service = ?
ORDER BY trace_id;
