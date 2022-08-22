SELECT trace_id, data FROM spans
WHERE trace_id IN rarray(?)
ORDER BY trace_id;
