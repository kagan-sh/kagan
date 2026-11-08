use gpui_shell::{HostArguments, HostError, HostModule};
use kg_domain::{BrowserOp, Op, ThreadId};

use crate::area::{argument, dispatch, error, from_host};

type Build = fn(&HostArguments) -> Result<BrowserOp, HostError>;

pub(crate) const CALLS: [(&str, Build); 14] = [
    ("open", |a| {
        Ok(BrowserOp::Open {
            thread_id: thread_id(a)?,
            url: a.string(1)?.to_owned(),
        })
    }),
    ("status", |a| {
        Ok(BrowserOp::Status {
            thread_id: thread_id(a)?,
        })
    }),
    ("snapshot", |a| {
        Ok(BrowserOp::Snapshot {
            thread_id: thread_id(a)?,
        })
    }),
    ("goto", |a| {
        Ok(BrowserOp::Goto {
            thread_id: thread_id(a)?,
            url: a.string(1)?.to_owned(),
        })
    }),
    ("back", |a| {
        Ok(BrowserOp::Back {
            thread_id: thread_id(a)?,
        })
    }),
    ("forward", |a| {
        Ok(BrowserOp::Forward {
            thread_id: thread_id(a)?,
        })
    }),
    ("reload", |a| {
        Ok(BrowserOp::Reload {
            thread_id: thread_id(a)?,
        })
    }),
    ("click", |a| {
        Ok(BrowserOp::Click {
            thread_id: thread_id(a)?,
            ref_id: whole(a, 1)?,
        })
    }),
    ("fill", |a| {
        Ok(BrowserOp::Fill {
            thread_id: thread_id(a)?,
            ref_id: whole(a, 1)?,
            text: Some(a.string(2)?.to_owned()),
            file: None,
        })
    }),
    ("type", |a| {
        Ok(BrowserOp::Type {
            thread_id: thread_id(a)?,
            text: a.string(1)?.to_owned(),
        })
    }),
    ("press", |a| {
        Ok(BrowserOp::Press {
            thread_id: thread_id(a)?,
            key: a.string(1)?.to_owned(),
        })
    }),
    ("select", |a| {
        Ok(BrowserOp::Select {
            thread_id: thread_id(a)?,
            ref_id: whole(a, 1)?,
            option: a.string(2)?.to_owned(),
        })
    }),
    ("scroll", |a| {
        Ok(BrowserOp::Scroll {
            thread_id: thread_id(a)?,
            dy: distance(a, 1)?,
        })
    }),
    ("close", |a| {
        Ok(BrowserOp::Close {
            thread_id: thread_id(a)?,
        })
    }),
];

pub(crate) fn module() -> HostModule {
    let mut module = HostModule::new("kagan/browser").declarations(include_str!("browser.d.ts"));
    for (name, build) in CALLS {
        module = module.async_function(name, move |args| dispatch(Op::Browser(build(args)?)));
    }
    module
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
