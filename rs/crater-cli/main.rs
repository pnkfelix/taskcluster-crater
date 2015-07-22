extern crate backtrace;
extern crate hyper;
extern crate rustc_serialize;
#[macro_use]
extern crate log;
extern crate env_logger;
extern crate crater_api as api;

use rustc_serialize::json;
use std::convert::From;
use std::env;
use std::error::Error as StdError;
use std::fmt::{self, Display, Formatter};
use std::fs::File;
use std::io::{self, Read};
use api::v1;
use std::io::Write;

enum Opts {
    CustomBuild { repo_url: String, commit_sha: String },
    CrateBuild { toolchain: String },
    Report { kind: v1::ReportKind },
    SelfTest
}

#[derive(RustcEncodable, RustcDecodable)]
pub struct Config {
    server_url: String,
    username: String,
    auth_token: String
}

fn main() {
    match run() {
        Ok(()) => {}
        Err(e) => {
            panic!("{:#?}", e);
        }
    }
}

fn run() -> Result<(), Error> {
    try!(env_logger::init());

    let config = try!(load_config());

    let ref args: Vec<String> = env::args().collect();
    let opts = try!(parse_opts(args));

    try!(run_run(config, opts));

    Ok(())
}

fn load_config() -> Result<Config, Error> {
    let mut path = try!(::std::env::current_dir());
    path.push("crater-cli-config.json");

    let mut file = try!(File::open(path));

    let mut s = String::new();
    try!(file.read_to_string(&mut s));

    return Ok(try!(json::decode(&s)));
}

fn parse_opts(args: &[String]) -> Result<Opts, Error> {
    if args.len() < 2 { return Err(Error::OptParse) }

    if args[1] == "custom-build" {
        let repo_url = try!(args.get(2).ok_or(Error::OptParse));
        let commit_sha = try!(args.get(3).ok_or(Error::OptParse));
        Ok(Opts::CustomBuild { repo_url: repo_url.clone(),
                               commit_sha: commit_sha.clone() })
    } else if args[1] == "crate-build" {
        let toolchain = try!(args.get(2).ok_or(Error::OptParse));
        Ok(Opts::CrateBuild { toolchain: toolchain.clone() })
    } else if args[1] == "report" {
        let ref kind = try!(args.get(2).ok_or(Error::OptParse));
        let kind = try!(parse_report_kind(kind, &args[3..]));
        Ok(Opts::Report { kind: kind })
    } else if args[1] == "self-test" {
        Ok(Opts::SelfTest)
    } else {
        Err(Error::OptParse)
    }
}

fn parse_report_kind(kind: &str, args: &[String]) -> Result<v1::ReportKind, Error> {
    if kind == "comparison" {
        let from = try!(args.get(0).ok_or(Error::OptParse));
        let to = try!(args.get(1).ok_or(Error::OptParse));
        Ok(v1::ReportKind::Comparison { toolchain_from: from.clone(),
                                        toolchain_to: to.clone() })
    } else if kind == "toolchain" {
        let toolchain = try!(args.get(0).ok_or(Error::OptParse));
        Ok(v1::ReportKind::Toolchain(toolchain.clone()))
    } else {
        Err(Error::OptParse)
    }
}

fn run_run(config: Config, opts: Opts) -> Result<(), Error> {
    let client_v1 = client_v1::Ctxt::new(config);
    let res = match opts {
        Opts::CustomBuild { repo_url, commit_sha } => {
            client_v1.custom_build(repo_url, commit_sha)
        }
        Opts::CrateBuild { toolchain } => {
            client_v1.crate_build(toolchain)
        }
        Opts::Report { kind } => {
            client_v1.report(kind)
        }
        Opts::SelfTest => {
            client_v1.self_test()
        }
    };

    match res {
        Ok(s) => {
            println!("{}", s);
            Ok(())
        }
        Err(Error::StdIoError(ref e, _)) => {
            try!(writeln!(std::io::stderr(),
                     "server reported an error executing node.js process"));
            try!(writeln!(std::io::stderr(), ""));
            try!(writeln!(std::io::stderr(), "{}", e.stderr));
            Ok(())
        }
        Err(e) => Err(e)
    }
}

#[derive(Debug)]
enum Error {
    OptParse,
    StdError(Box<StdError + Send>, StackTrace),
    StdIoError(v1::StdIoResponse, StackTrace)
}

#[derive(Debug)]
struct StackTrace {
    frames: Vec<StackFrame>,
}

use std::os::raw::c_void;

#[derive(Debug)]
struct StackFrame {
    ip: *mut c_void,
    sym: Option<Sym>,
}

#[derive(Debug)]
struct Sym {
    name: Option<String>,
    addr: Option<*mut c_void>,
    filename: Option<String>,
    lineno: Option<u32>,
}

fn capture_stacktrace() -> StackTrace {
    let mut frames = Vec::new();
    backtrace::trace(&mut |frame| {
        let ip = frame.ip();
        let mut sym: Option<Sym> = None;

        backtrace::resolve(ip, &mut |symbol| {
            let mut new_sym = Sym {
                name: None, addr: None, filename: None, lineno: None
            };
            if let Some(name) = symbol.name() {
                let name = String::from_utf8_lossy(name).into_owned();
                new_sym.name = Some(name);
            }
            if let Some(addr) = symbol.addr() {
                new_sym.addr = Some(addr);
            }
            if let Some(filename) = symbol.filename() {
                let filename = String::from_utf8_lossy(filename).into_owned();
                new_sym.filename = Some(filename);
            }
            if let Some(lineno) = symbol.lineno() {
                new_sym.lineno = Some(lineno);
            }

            sym = Some(new_sym);
        });

        frames.push(StackFrame { ip: ip, sym: sym });
        true // keep going to the next frame
    });

    StackTrace { frames: frames }
}

impl StdError for Error {
    fn description(&self) -> &str {
        match *self {
            Error::OptParse => "bad arguments",
            Error::StdError(ref e, _) => e.description(),
            Error::StdIoError(ref e, _) => &*e.stderr
        }
    }

    fn cause(&self) -> Option<&StdError> {
        match *self {
            Error::StdError(ref e, _) => Some(&**e),
            _ => None
        }
    }
}

impl Display for Error {
    fn fmt(&self, f: &mut Formatter) -> Result<(), fmt::Error> {
        f.write_str(self.description())
    }
}


impl From<io::Error> for Error {
    fn from(e: io::Error) -> Error {
        Error::StdError(Box::new(e), capture_stacktrace())
    }
}

impl From<json::DecoderError> for Error {
    fn from(e: json::DecoderError) -> Error {
        Error::StdError(Box::new(e), capture_stacktrace())
    }
}

impl From<json::EncoderError> for Error {
    fn from(e: json::EncoderError) -> Error {
        Error::StdError(Box::new(e), capture_stacktrace())
    }
}

impl From<hyper::Error> for Error {
    fn from(e: hyper::Error) -> Error {
        Error::StdError(Box::new(e), capture_stacktrace())
    }
}

impl From<log::SetLoggerError> for Error {
    fn from(e: log::SetLoggerError) -> Error {
        Error::StdError(Box::new(e), capture_stacktrace())
    }
}

impl From<v1::StdIoResponse> for Error {
    fn from(e: v1::StdIoResponse) -> Error {
        Error::StdIoError(e, capture_stacktrace())
    }
}

mod client_v1 {
    use super::{Config, Error};
    use hyper::Client;
    use api::v1;
    use rustc_serialize::json;
    use std::io::Read;
    use rustc_serialize::Encodable;

    pub struct Ctxt {
        config: Config
    }

    impl Ctxt {
        pub fn new(config: Config) -> Ctxt {
            Ctxt { config: config }
        }

        /// Returns the stdout from `node schedule-tasks.js custom-build`
        pub fn custom_build(&self, repo_url: String, commit_sha: String) -> Result<String, Error> {
            let req = v1::CustomBuildRequest {
                auth: self.auth(),
                repo_url: repo_url, commit_sha: commit_sha
            };
            stdio_req(&self.config, "custom_build", req)
        }

        pub fn crate_build(&self, toolchain: String) -> Result<String, Error> {
            let req = v1::CrateBuildRequest {
                auth: self.auth(),
                toolchain: toolchain
            };
            stdio_req(&self.config, "crate_build", req)
        }

        pub fn report(&self, kind: v1::ReportKind) -> Result<String, Error> {
            let req = v1::ReportRequest {
                auth: self.auth(),
                kind: kind
            };
            stdio_req(&self.config, "report", req)
        }

        pub fn self_test(&self) -> Result<String, Error> {
            let req = v1::SelfTestRequest {
                auth: self.auth()
            };
            stdio_req(&self.config, "self-test", req)
        }

        fn auth(&self) -> v1::Auth {
            v1::Auth {
                name: self.config.username.clone(),
                token: self.config.auth_token.clone()
            }
        }
    }

    fn stdio_req<T>(config: &Config, name: &str, ref req: T) -> Result<String, Error>
        where T: Encodable {
        let ref api_url = format!("{}/api/v1/{}", config.server_url, name);
        info!("api endpoint: {}", api_url);
        let ref req_str = try!(json::encode(req));

        let mut client = Client::new();
        let mut http_res = try!(client.post(api_url).body(req_str).send());
        let ref mut res_str = String::new();
        try!(http_res.read_to_string(res_str));

        let res: v1::StdIoResponse = try!(json::decode(res_str));
        let stdout = try!(Result::from(res));

        Ok(stdout)
    }
}
