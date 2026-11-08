//! `kagan/browser`: the same verbs as `kg browser`, for a plugin that names the thread.

use gpui_shell::{HostArguments, HostError, HostModule};
use kg_domain::{BrowserOp, Op, ThreadId};

use crate::area::{argument, dispatch, error, from_host};

const CALLS: [&str; 14] = [
    "open", "status", "snapshot", "goto", "back", "forward", "reload", "click", "fill", "type",
    "press", "select", "scroll", "close",
];

pub(crate) fn module() -> HostModule {
    let mut module = HostModule::new("kagan/browser").declarations(include_str!("browser.d.ts"));
    for name in CALLS {
        module = module.async_function(name, move |args| dispatch(operation(name, args)?));
    }
    module
}

fn operation(name: &str, args: &HostArguments) -> Result<Op, HostError> {
    let op = match name {
        "open" => BrowserOp::Open {
            thread_id: thread_id(args)?,
            url: args.string(1)?.to_owned(),
        },
        "status" => BrowserOp::Status {
            thread_id: thread_id(args)?,
        },
        "snapshot" => BrowserOp::Snapshot {
            thread_id: thread_id(args)?,
        },
        "goto" => BrowserOp::Goto {
            thread_id: thread_id(args)?,
            url: args.string(1)?.to_owned(),
        },
        "back" => BrowserOp::Back {
            thread_id: thread_id(args)?,
        },
        "forward" => BrowserOp::Forward {
            thread_id: thread_id(args)?,
        },
        "reload" => BrowserOp::Reload {
            thread_id: thread_id(args)?,
        },
        "click" => BrowserOp::Click {
            thread_id: thread_id(args)?,
            ref_id: whole(args, 1)?,
        },
        "fill" => BrowserOp::Fill {
            thread_id: thread_id(args)?,
            ref_id: whole(args, 1)?,
            text: Some(args.string(2)?.to_owned()),
            file: None,
        },
        "type" => BrowserOp::Type {
            thread_id: thread_id(args)?,
            text: args.string(1)?.to_owned(),
        },
        "press" => BrowserOp::Press {
            thread_id: thread_id(args)?,
            key: args.string(1)?.to_owned(),
        },
        "select" => BrowserOp::Select {
            thread_id: thread_id(args)?,
            ref_id: whole(args, 1)?,
            option: args.string(2)?.to_owned(),
        },
        "scroll" => BrowserOp::Scroll {
            thread_id: thread_id(args)?,
            dy: distance(args, 1)?,
        },
        "close" => BrowserOp::Close {
            thread_id: thread_id(args)?,
        },
        _ => return Err(error(format!("unknown browser function {name}"))),
    };
    Ok(Op::Browser(op))
}

fn thread_id(args: &HostArguments) -> Result<ThreadId, HostError> {
    Ok(ThreadId(args.string(0)?.to_owned()))
}

fn whole(args: &HostArguments, ix: usize) -> Result<u32, HostError> {
    let value = from_host(args.value(ix)?);
    let Some(number) = value.as_u64() else {
        return Err(error("ref must be a whole number"));
    };
    u32::try_from(number).map_err(|_| error("ref must be a whole number"))
}

fn distance(args: &HostArguments, ix: usize) -> Result<f32, HostError> {
    let dy = argument::<f32>(args, ix).map_err(|_| error("scroll must be a number"))?;
    if !dy.is_finite() {
        return Err(error("scroll must be a number"));
    }
    Ok(dy)
}
