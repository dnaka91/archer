pub mod agent {
    use archer_thrift_derive::ThriftDeserialize;
    use thrift::{
        protocol::{TInputProtocol, TOutputProtocol},
        ApplicationError, ApplicationErrorKind,
    };

    use crate::ThriftDeserialize;

    use super::jaeger::Batch;

    pub trait AgentSyncHandler {
        fn handle_emit_batch(&self, batch: Batch) -> thrift::Result<()>;
    }

    pub struct AgentSyncProcessor<T>(T);

    impl<T: AgentSyncHandler> AgentSyncProcessor<T> {
        pub fn new(handler: T) -> Self {
            Self(handler)
        }

        pub fn process(
            &self,
            input: &mut impl TInputProtocol,
            output: &mut impl TOutputProtocol,
        ) -> thrift::Result<()> {
            let ident = input.read_message_begin()?;
            let result = match ident.name.as_str() {
                "emitBatch" => self.process_emit_batch(input),
                method => Err(thrift::Error::Application(ApplicationError::new(
                    ApplicationErrorKind::UnknownMethod,
                    format!("unknown method {method}"),
                ))),
            };

            thrift::server::handle_process_result(&ident, result, output)
        }

        fn process_emit_batch(&self, input: &mut impl TInputProtocol) -> thrift::Result<()> {
            let args = AgentEmitBatchArgs::read(input)?;

            self.0.handle_emit_batch(args.batch).map_err(|e| match e {
                thrift::Error::Application(err) => thrift::Error::Application(err),
                _ => thrift::Error::Application(ApplicationError::new(
                    ApplicationErrorKind::Unknown,
                    e.to_string(),
                )),
            })
        }
    }

    #[derive(Default, ThriftDeserialize)]
    struct AgentEmitBatchArgs {
        batch: Batch,
    }
}

pub mod jaeger {
    use archer_thrift_derive::ThriftDeserialize;
    use thrift::{
        protocol::{self, TInputProtocol, TType},
        server::TProcessor,
        ProtocolError, ProtocolErrorKind,
    };

    use crate::ThriftDeserialize;

    #[derive(Clone, Copy, Debug, Default, ThriftDeserialize)]
    pub enum TagType {
        #[default]
        String,
        Double,
        Bool,
        Long,
        Binary,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct Tag {
        pub key: String,
        pub v_type: TagType,
        pub v_str: Option<String>,
        pub v_double: Option<f64>,
        pub v_bool: Option<bool>,
        pub v_long: Option<i64>,
        pub v_binary: Option<Vec<u8>>,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct Log {
        pub timestamp: i64,
        pub fields: Vec<Tag>,
    }

    #[derive(Clone, Copy, Debug, Default, ThriftDeserialize)]
    pub enum SpanRefType {
        #[default]
        ChildOf,
        FollowsFrom,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct SpanRef {
        pub ref_type: SpanRefType,
        pub trace_id_low: i64,
        pub trace_id_high: i64,
        pub span_id: i64,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct Span {
        pub trace_id_low: i64,
        pub trace_id_high: i64,
        pub span_id: i64,
        pub parent_span_id: i64,
        pub operation_name: String,
        pub references: Option<Vec<SpanRef>>,
        pub flags: i32,
        pub start_time: i64,
        pub duration: i64,
        pub tags: Option<Vec<Tag>>,
        pub logs: Option<Vec<Log>>,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct Process {
        pub service_name: String,
        pub tags: Option<Vec<Tag>>,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct ClientStats {
        pub full_queue_dropped_spans: i64,
        pub too_large_dropped_spans: i64,
        pub failed_to_emit_spans: i64,
    }

    #[derive(Clone, Debug, Default, ThriftDeserialize)]
    pub struct Batch {
        pub process: Process,
        pub spans: Vec<Span>,
        pub seq_no: Option<i64>,
        pub stats: Option<ClientStats>,
    }

    pub struct BatchSubmitResponse {
        pub ok: bool,
    }

    pub trait Collector {
        fn submit_batches(batches: Vec<Batch>) -> Vec<BatchSubmitResponse>;
    }

    pub(crate) fn verify_read(field: &str, read: bool) -> thrift::Result<()> {
        match read {
            true => Ok(()),
            false => Err(thrift::Error::Protocol(ProtocolError::new(
                ProtocolErrorKind::InvalidData,
                format!("missing required field {field}"),
            ))),
        }
    }

    pub(crate) fn read_list<T: ThriftDeserialize>(
        prot: &mut impl TInputProtocol,
    ) -> thrift::Result<Vec<T>> {
        let ident = prot.read_list_begin()?;
        let fields = (0..ident.size)
            .map(|_| T::read(prot))
            .collect::<thrift::Result<_>>()?;

        prot.read_list_end()?;
        Ok(fields)
    }

    pub fn read_batch(prot: &mut impl TInputProtocol) -> thrift::Result<Batch> {
        Batch::read(prot)
    }
}
