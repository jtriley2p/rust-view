//! Typed data flow: the types each fn takes and gives, read from signatures.

use crate::modules::{Place, scoped};
use crate::report::{Flow, FlowKind, Hop, Mode, Port};
use crate::resolve::{Index, Named};
use crate::scan::{Scan, Ty};
use std::collections::HashMap;

/// Types that stand for their type arguments, matched by name. `Result` does too, with its
/// second as an error.
const WRAPPERS: &[&str] = &["Option", "Vec", "Box", "Rc", "Arc", "Pin", "Poll"];
/// Channel ends, matched by name.
const SENDERS: &[&str] = &["Sender", "SyncSender", "UnboundedSender"];
const RECEIVERS: &[&str] = &["Receiver", "UnboundedReceiver"];
/// How many aliases deep a type is expanded.
const DEPTH: u8 = 8;

/// A type a signature names, once wrappers are taken off.
#[derive(Clone)]
struct Leaf {
    ty: Hop,
    /// How the nearest reference around it takes it; None if by value.
    by: Option<Mode>,
    /// The `E` of a `Result`.
    error: bool,
    /// Inside a channel end.
    channel: bool,
    /// Inside a sender: values go in where the sender is taken.
    sent: bool,
}

impl Leaf {
    fn new(ty: Hop) -> Self {
        Leaf {
            ty,
            by: None,
            error: false,
            channel: false,
            sent: false,
        }
    }

    /// What crosses a channel is moved, however the channel end is held.
    fn port(self) -> Port {
        Port {
            ty: self.ty,
            by: self.by.filter(|_| !self.channel).unwrap_or(Mode::Move),
            error: self.error,
            channel: self.channel,
        }
    }
}

/// Names a signature binds: a type parameter to nothing, `Self` and an alias's parameters to
/// their types.
type Env<'a> = HashMap<&'a str, Vec<Leaf>>;

struct Scope<'a> {
    index: &'a Index<'a>,
    root: usize,
}

impl Scope<'_> {
    /// The types `ty` names where it is written in `module`.
    fn leaves(&self, module: &str, ty: &Ty, env: &Env, depth: u8) -> Vec<Leaf> {
        let (path, args) = match ty {
            Ty::Path { path, args } => (path, args),
            Ty::Ref { mutable, to } => {
                let by = match mutable {
                    true => Mode::Mut,
                    false => Mode::Read,
                };
                let inner = self.leaves(module, to, env, depth).into_iter();
                let taken = inner.map(|l| Leaf {
                    by: l.by.or(Some(by)),
                    ..l
                });
                return taken.collect();
            }
            Ty::Group(parts) => {
                let each = parts.iter().map(|t| self.leaves(module, t, env, depth));
                return each.flatten().collect();
            }
        };
        let (Some(first), Some(last)) = (path.first(), path.last()) else {
            return Vec::new();
        };
        if let Some(bound) = env.get(first.as_str()) {
            // `T::Item` names nothing known.
            return bound.iter().filter(|_| path.len() == 1).cloned().collect();
        }
        let arg = |n: usize| {
            let ty = args.get(n).map(|t| self.leaves(module, t, env, depth));
            ty.unwrap_or_default().into_iter()
        };
        let named = self.index.named(self.root, module, path);
        // A wrapper is matched by the name it was imported under: `use .. Result as StdResult`.
        let name = match &named {
            Some(Named::External(ty)) => ty.name.clone(),
            _ => last.clone(),
        };
        match (named, name.as_str()) {
            (Some(Named::Type(ty)), _) => vec![Leaf::new(ty)],
            (Some(Named::Alias(home, alias)), _) if depth > 0 => {
                let params = alias.generics.iter().enumerate();
                let env = params
                    .map(|(n, g)| (g.as_str(), arg(n).collect()))
                    .collect();
                self.leaves(&home, &alias.ty, &env, depth - 1)
            }
            (Some(Named::Alias(..)), _) => Vec::new(),
            (_, "Result") => {
                let error = arg(1).map(|l| Leaf { error: true, ..l });
                arg(0).chain(error).collect()
            }
            (_, name) if WRAPPERS.contains(&name) => (0..args.len()).flat_map(arg).collect(),
            (_, name) if SENDERS.contains(&name) => {
                let sent = arg(0).map(|l| Leaf {
                    channel: true,
                    sent: true,
                    ..l
                });
                sent.collect()
            }
            (_, name) if RECEIVERS.contains(&name) => {
                arg(0).map(|l| Leaf { channel: true, ..l }).collect()
            }
            (Some(Named::External(ty)), _) => vec![Leaf::new(ty)],
            (None, _) => Vec::new(),
        }
    }
}

/// The file's flows: one per fn or conversion impl that names a type, and per channel end a
/// field holds.
pub fn flows(index: &Index, path: &str, place: &Place, scan: &Scan) -> Vec<Flow> {
    let scope = Scope {
        index,
        root: place.root,
    };
    let add = |ports: &mut Vec<Port>, leaf: Leaf| {
        let port = leaf.port();
        if !ports.contains(&port) {
            ports.push(port);
        }
    };
    let mut out = Vec::new();

    for sig in &scan.sigs {
        let module = scoped(&place.module, &sig.scope);
        let mut env: Env = sig.generics.iter().map(|g| (g.as_str(), vec![])).collect();
        let leaves = |ty: &Ty, env: &Env| scope.leaves(&module, ty, env, DEPTH);
        let this = sig.this.as_ref().map(|ty| leaves(ty, &env));
        env.insert("Self", this.unwrap_or_default());

        let (mut inputs, mut outputs) = (Vec::new(), Vec::new());
        for leaf in sig.inputs.iter().flat_map(|ty| leaves(ty, &env)) {
            // What a sender carries leaves the fn through it.
            match leaf.sent {
                true => add(&mut outputs, leaf),
                false => add(&mut inputs, leaf),
            }
        }
        // A sender given back takes values later, somewhere else.
        let output = sig.output.iter().flat_map(|ty| leaves(ty, &env));
        let error = sig.error.iter().flat_map(|ty| leaves(ty, &env));
        let error = error.map(|l| Leaf { error: true, ..l });
        let given = output.filter(|l| !l.sent).chain(error);
        given.for_each(|l| add(&mut outputs, l));
        // A conversion with an end unnamed converts nothing known.
        let kept = match sig.conversion {
            true => !inputs.is_empty() && !outputs.is_empty(),
            false => !inputs.is_empty() || !outputs.is_empty(),
        };
        if kept {
            out.push(Flow {
                name: sig.name.clone(),
                module,
                kind: match sig.conversion {
                    true => FlowKind::Convert,
                    false => FlowKind::Fn,
                },
                inputs,
                outputs,
            });
        }
    }

    for field in &scan.fields {
        let module = scoped(&place.module, &field.scope);
        let owner = Leaf::new(Hop {
            module: module.clone(),
            file: Some(path.to_string()),
            name: field.owner.clone(),
        });
        let mut env: Env = field
            .generics
            .iter()
            .map(|g| (g.as_str(), vec![]))
            .collect();
        env.insert("Self", vec![owner.clone()]);
        let leaves = scope.leaves(&module, &field.ty, &env, DEPTH);
        for leaf in leaves.into_iter().filter(|l| l.channel) {
            // The type sends through a sender it holds, and takes what a receiver it holds gets.
            let (input, output) = match leaf.sent {
                true => (
                    Leaf {
                        by: Some(Mode::Read),
                        ..owner.clone()
                    },
                    leaf,
                ),
                false => (leaf, owner.clone()),
            };
            out.push(Flow {
                name: format!("{}.{}", field.owner, field.name),
                module: module.clone(),
                kind: FlowKind::Field,
                inputs: vec![input.port()],
                outputs: vec![output.port()],
            });
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use crate::{CrateInput, Source, analyze};

    const LIB: &str = "
        mod error;
        mod jobs;
        pub use error::Error;
        use std::result::Result as StdResult;
        pub type Result<T> = StdResult<T, Error>;
    ";
    const ERROR: &str = "
        use std::io;
        pub enum Error { Io(io::Error) }
        impl From<io::Error> for Error {
            fn from(e: io::Error) -> Self { Error::Io(e) }
        }
    ";
    const JOBS: &str = "
        use crate::{Error, Result};
        use std::sync::mpsc::{Receiver, Sender};
        use std::sync::Arc;

        pub struct Config;
        pub struct Job;
        pub struct Report;
        pub struct Queue { jobs: Vec<Job>, tx: Sender<Report> }
        pub struct Worker { rx: Receiver<Job> }
        pub enum Cmd { Get { reply: Sender<Report> }, Stop }

        impl Queue {
            pub fn push(&mut self, job: Job) {}
            pub fn pop(&mut self) -> Option<Job> { None }
            pub fn peek(&self) -> &[Job] { &self.jobs }
            pub fn find(&self, like: Option<&Job>, into: (&mut Report, Config)) {}
            pub fn load(config: &Config, name: String) -> Result<Self> { todo!() }
            pub fn share(self) -> Arc<Self> { Arc::new(self) }
            pub fn each<T: Into<Job>>(&self, t: T, f: impl Fn(&Job)) -> T { t }
        }
        pub fn produce(config: &Config, tx: Sender<Job>) {}
        pub fn work(rx: Receiver<Job>) -> (Report, Box<Config>) { todo!() }
        impl TryFrom<Job> for Report {
            type Error = Error;
            fn try_from(job: Job) -> Result<Self> { todo!() }
        }
        impl Into<Config> for Job {
            fn into(self) -> Config { Config }
        }
        fn outer() { fn local(job: Job) -> Report { todo!() } }
        #[cfg(test)]
        mod tests { fn check(job: super::Job) -> super::Report { todo!() } }
    ";

    #[test]
    fn flows() {
        let source = |path: &str, text: &str| Source {
            path: path.into(),
            text: text.into(),
        };
        let analysis = analyze(&[CrateInput {
            name: "k".into(),
            version: "0.1.0".into(),
            path: ".".into(),
            dependencies: vec![],
            packages: vec![],
            roots: vec!["src/lib.rs".into()],
            files: vec![
                source("src/lib.rs", LIB),
                source("src/error.rs", ERROR),
                source("src/jobs.rs", JOBS),
            ],
        }]);
        assert_eq!(analysis.errors, []);
        let [lib, error, jobs] = &analysis.report.crates[0].files[..] else {
            panic!()
        };
        let flows = |f: &crate::report::File| -> Vec<String> {
            let ports = |ports: &[crate::report::Port]| -> String {
                let port = |p: &crate::report::Port| {
                    let by = format!(" {:?}", p.by).to_lowercase().replace(" move", "");
                    let (error, channel) = (
                        if p.error { " error" } else { "" },
                        if p.channel { " channel" } else { "" },
                    );
                    format!("{}{by}{error}{channel}", p.ty.name)
                };
                ports.iter().map(port).collect::<Vec<_>>().join(", ")
            };
            let line = |f: &crate::report::Flow| {
                let kind = format!("{:?}", f.kind).to_lowercase();
                let (inputs, outputs) = (ports(&f.inputs), ports(&f.outputs));
                format!("{kind} {}: {inputs} > {outputs}", f.name)
            };
            f.flows.iter().map(line).collect()
        };
        assert!(lib.flows.is_empty());
        assert_eq!(flows(error), ["convert From: Error > Error"]);
        let from = &error.flows[0];
        let (input, output) = (&from.inputs[0].ty, &from.outputs[0].ty);
        assert_eq!(from.module, "crate::error");
        assert_eq!(
            (input.module.as_str(), input.file.as_deref()),
            ("std::io", None)
        );
        assert_eq!(
            (output.module.as_str(), output.file.as_deref()),
            ("crate::error", Some("src/error.rs"))
        );
        assert_eq!(
            flows(jobs),
            [
                "fn Queue::push: Queue mut, Job > ",
                "fn Queue::pop: Queue mut > Job",
                "fn Queue::peek: Queue read > Job read",
                // The nearest reference decides how a type is taken.
                "fn Queue::find: Queue read, Job read, Report mut, Config > ",
                // The alias is expanded where it is defined, to a wrapper under its own name;
                // `String` is no type.
                "fn Queue::load: Config read > Queue, Error error",
                "fn Queue::share: Queue > Queue",
                // A type parameter and an `impl Trait` name no type.
                "fn Queue::each: Queue read > ",
                // What a sender is given leaves the fn; `outer` names no type and is left out.
                "fn produce: Config read > Job channel",
                "fn work: Job channel > Report, Config",
                "convert TryFrom: Job > Report, Error error",
                "convert Into: Job > Config",
                "field Queue.tx: Queue read > Report channel",
                "field Worker.rx: Job channel > Worker",
                "field Cmd.Get.reply: Cmd read > Report channel",
            ]
        );
        let error = &jobs.flows[4].outputs[1].ty;
        assert_eq!(
            (error.module.as_str(), error.file.as_deref()),
            ("crate::error", Some("src/error.rs"))
        );
    }
}
