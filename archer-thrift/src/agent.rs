// Autogenerated by Thrift Compiler (0.16.0)
// DO NOT EDIT UNLESS YOU ARE SURE THAT YOU KNOW WHAT YOU ARE DOING

#![allow(unused_imports)]
#![allow(unused_extern_crates)]
#![allow(clippy::too_many_arguments, clippy::type_complexity, clippy::vec_box)]
#![cfg_attr(rustfmt, rustfmt_skip)]

use std::cell::RefCell;
use std::collections::{BTreeMap, BTreeSet};
use std::convert::{From, TryFrom};
use std::default::Default;
use std::error::Error;
use std::fmt;
use std::fmt::{Display, Formatter};
use std::rc::Rc;

use thrift::OrderedFloat;
use thrift::{ApplicationError, ApplicationErrorKind, ProtocolError, ProtocolErrorKind, TThriftClient};
use thrift::protocol::{TFieldIdentifier, TListIdentifier, TMapIdentifier, TMessageIdentifier, TMessageType, TInputProtocol, TOutputProtocol, TSetIdentifier, TStructIdentifier, TType};
use thrift::protocol::field_id;
use thrift::protocol::verify_expected_message_type;
use thrift::protocol::verify_expected_sequence_number;
use thrift::protocol::verify_expected_service_call;
use thrift::protocol::verify_required_field_exists;
use thrift::server::TProcessor;

use crate::jaeger;
use crate::zipkincore;

//
// Agent service client
//

pub trait TAgentSyncClient {
  fn emit_zipkin_batch(&mut self, spans: Vec<zipkincore::Span>) -> thrift::Result<()>;
  fn emit_batch(&mut self, batch: jaeger::Batch) -> thrift::Result<()>;
}

pub trait TAgentSyncClientMarker {}

pub struct AgentSyncClient<IP, OP> where IP: TInputProtocol, OP: TOutputProtocol {
  _i_prot: IP,
  _o_prot: OP,
  _sequence_number: i32,
}

impl <IP, OP> AgentSyncClient<IP, OP> where IP: TInputProtocol, OP: TOutputProtocol {
  pub fn new(input_protocol: IP, output_protocol: OP) -> AgentSyncClient<IP, OP> {
    AgentSyncClient { _i_prot: input_protocol, _o_prot: output_protocol, _sequence_number: 0 }
  }
}

impl <IP, OP> TThriftClient for AgentSyncClient<IP, OP> where IP: TInputProtocol, OP: TOutputProtocol {
  fn i_prot_mut(&mut self) -> &mut dyn TInputProtocol { &mut self._i_prot }
  fn o_prot_mut(&mut self) -> &mut dyn TOutputProtocol { &mut self._o_prot }
  fn sequence_number(&self) -> i32 { self._sequence_number }
  fn increment_sequence_number(&mut self) -> i32 { self._sequence_number += 1; self._sequence_number }
}

impl <IP, OP> TAgentSyncClientMarker for AgentSyncClient<IP, OP> where IP: TInputProtocol, OP: TOutputProtocol {}

impl <C: TThriftClient + TAgentSyncClientMarker> TAgentSyncClient for C {
  fn emit_zipkin_batch(&mut self, spans: Vec<zipkincore::Span>) -> thrift::Result<()> {
    (
      {
        self.increment_sequence_number();
        let message_ident = TMessageIdentifier::new("emitZipkinBatch", TMessageType::OneWay, self.sequence_number());
        let call_args = AgentEmitZipkinBatchArgs { spans };
        self.o_prot_mut().write_message_begin(&message_ident)?;
        call_args.write_to_out_protocol(self.o_prot_mut())?;
        self.o_prot_mut().write_message_end()?;
        self.o_prot_mut().flush()
      }
    )?;
    Ok(())
  }
  fn emit_batch(&mut self, batch: jaeger::Batch) -> thrift::Result<()> {
    (
      {
        self.increment_sequence_number();
        let message_ident = TMessageIdentifier::new("emitBatch", TMessageType::OneWay, self.sequence_number());
        let call_args = AgentEmitBatchArgs { batch };
        self.o_prot_mut().write_message_begin(&message_ident)?;
        call_args.write_to_out_protocol(self.o_prot_mut())?;
        self.o_prot_mut().write_message_end()?;
        self.o_prot_mut().flush()
      }
    )?;
    Ok(())
  }
}

//
// Agent service processor
//

pub trait AgentSyncHandler {
  fn handle_emit_zipkin_batch(&self, spans: Vec<zipkincore::Span>) -> thrift::Result<()>;
  fn handle_emit_batch(&self, batch: jaeger::Batch) -> thrift::Result<()>;
}

pub struct AgentSyncProcessor<H: AgentSyncHandler> {
  handler: H,
}

impl <H: AgentSyncHandler> AgentSyncProcessor<H> {
  pub fn new(handler: H) -> AgentSyncProcessor<H> {
    AgentSyncProcessor {
      handler,
    }
  }
  fn process_emit_zipkin_batch(&self, incoming_sequence_number: i32, i_prot: &mut dyn TInputProtocol, o_prot: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    TAgentProcessFunctions::process_emit_zipkin_batch(&self.handler, incoming_sequence_number, i_prot, o_prot)
  }
  fn process_emit_batch(&self, incoming_sequence_number: i32, i_prot: &mut dyn TInputProtocol, o_prot: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    TAgentProcessFunctions::process_emit_batch(&self.handler, incoming_sequence_number, i_prot, o_prot)
  }
}

pub struct TAgentProcessFunctions;

impl TAgentProcessFunctions {
  pub fn process_emit_zipkin_batch<H: AgentSyncHandler>(handler: &H, _: i32, i_prot: &mut dyn TInputProtocol, _: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    let args = AgentEmitZipkinBatchArgs::read_from_in_protocol(i_prot)?;
    match handler.handle_emit_zipkin_batch(args.spans) {
      Ok(_) => {
        Ok(())
      },
      Err(e) => {
        match e {
          thrift::Error::Application(app_err) => {
            Err(thrift::Error::Application(app_err))
          },
          _ => {
            let ret_err = {
              ApplicationError::new(
                ApplicationErrorKind::Unknown,
                e.to_string()
              )
            };
            Err(thrift::Error::Application(ret_err))
          },
        }
      },
    }
  }
  pub fn process_emit_batch<H: AgentSyncHandler>(handler: &H, _: i32, i_prot: &mut dyn TInputProtocol, _: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    let args = AgentEmitBatchArgs::read_from_in_protocol(i_prot)?;
    match handler.handle_emit_batch(args.batch) {
      Ok(_) => {
        Ok(())
      },
      Err(e) => {
        match e {
          thrift::Error::Application(app_err) => {
            Err(thrift::Error::Application(app_err))
          },
          _ => {
            let ret_err = {
              ApplicationError::new(
                ApplicationErrorKind::Unknown,
                e.to_string()
              )
            };
            Err(thrift::Error::Application(ret_err))
          },
        }
      },
    }
  }
}

impl <H: AgentSyncHandler> TProcessor for AgentSyncProcessor<H> {
  fn process(&self, i_prot: &mut dyn TInputProtocol, o_prot: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    let message_ident = i_prot.read_message_begin()?;
    let res = match &*message_ident.name {
      "emitZipkinBatch" => {
        self.process_emit_zipkin_batch(message_ident.sequence_number, i_prot, o_prot)
      },
      "emitBatch" => {
        self.process_emit_batch(message_ident.sequence_number, i_prot, o_prot)
      },
      method => {
        Err(
          thrift::Error::Application(
            ApplicationError::new(
              ApplicationErrorKind::UnknownMethod,
              format!("unknown method {}", method)
            )
          )
        )
      },
    };
    thrift::server::handle_process_result(&message_ident, res, o_prot)
  }
}

//
// AgentEmitZipkinBatchArgs
//

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AgentEmitZipkinBatchArgs {
  spans: Vec<zipkincore::Span>,
}

impl AgentEmitZipkinBatchArgs {
  fn read_from_in_protocol(i_prot: &mut dyn TInputProtocol) -> thrift::Result<AgentEmitZipkinBatchArgs> {
    i_prot.read_struct_begin()?;
    let mut f_1: Option<Vec<zipkincore::Span>> = None;
    loop {
      let field_ident = i_prot.read_field_begin()?;
      if field_ident.field_type == TType::Stop {
        break;
      }
      let field_id = field_id(&field_ident)?;
      match field_id {
        1 => {
          let list_ident = i_prot.read_list_begin()?;
          let mut val: Vec<zipkincore::Span> = Vec::with_capacity(list_ident.size as usize);
          for _ in 0..list_ident.size {
            let list_elem_0 = zipkincore::Span::read_from_in_protocol(i_prot)?;
            val.push(list_elem_0);
          }
          i_prot.read_list_end()?;
          f_1 = Some(val);
        },
        _ => {
          i_prot.skip(field_ident.field_type)?;
        },
      };
      i_prot.read_field_end()?;
    }
    i_prot.read_struct_end()?;
    verify_required_field_exists("AgentEmitZipkinBatchArgs.spans", &f_1)?;
    let ret = AgentEmitZipkinBatchArgs {
      spans: f_1.expect("auto-generated code should have checked for presence of required fields"),
    };
    Ok(ret)
  }
  fn write_to_out_protocol(&self, o_prot: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    let struct_ident = TStructIdentifier::new("emitZipkinBatch_args");
    o_prot.write_struct_begin(&struct_ident)?;
    o_prot.write_field_begin(&TFieldIdentifier::new("spans", TType::List, 1))?;
    o_prot.write_list_begin(&TListIdentifier::new(TType::Struct, self.spans.len() as i32))?;
    for e in &self.spans {
      e.write_to_out_protocol(o_prot)?;
    }
    o_prot.write_list_end()?;
    o_prot.write_field_end()?;
    o_prot.write_field_stop()?;
    o_prot.write_struct_end()
  }
}

//
// AgentEmitBatchArgs
//

#[derive(Clone, Debug, Eq, Hash, Ord, PartialEq, PartialOrd)]
struct AgentEmitBatchArgs {
  batch: jaeger::Batch,
}

impl AgentEmitBatchArgs {
  fn read_from_in_protocol(i_prot: &mut dyn TInputProtocol) -> thrift::Result<AgentEmitBatchArgs> {
    i_prot.read_struct_begin()?;
    let mut f_1: Option<jaeger::Batch> = None;
    loop {
      let field_ident = i_prot.read_field_begin()?;
      if field_ident.field_type == TType::Stop {
        break;
      }
      let field_id = field_id(&field_ident)?;
      match field_id {
        1 => {
          let val = jaeger::Batch::read_from_in_protocol(i_prot)?;
          f_1 = Some(val);
        },
        _ => {
          i_prot.skip(field_ident.field_type)?;
        },
      };
      i_prot.read_field_end()?;
    }
    i_prot.read_struct_end()?;
    verify_required_field_exists("AgentEmitBatchArgs.batch", &f_1)?;
    let ret = AgentEmitBatchArgs {
      batch: f_1.expect("auto-generated code should have checked for presence of required fields"),
    };
    Ok(ret)
  }
  fn write_to_out_protocol(&self, o_prot: &mut dyn TOutputProtocol) -> thrift::Result<()> {
    let struct_ident = TStructIdentifier::new("emitBatch_args");
    o_prot.write_struct_begin(&struct_ident)?;
    o_prot.write_field_begin(&TFieldIdentifier::new("batch", TType::Struct, 1))?;
    self.batch.write_to_out_protocol(o_prot)?;
    o_prot.write_field_end()?;
    o_prot.write_field_stop()?;
    o_prot.write_struct_end()
  }
}

