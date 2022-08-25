SELECT DISTINCT spans.operation FROM spans
JOIN traces ON traces.trace_id = spans.span_id
WHERE traces.service = ?;
