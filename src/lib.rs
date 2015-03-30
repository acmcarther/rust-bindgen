#![crate_name = "bindgen"]
#![crate_type = "dylib"]
#![feature(quote, plugin_registrar, unboxed_closures, rustc_private, libc, core)]

extern crate syntax;
extern crate rustc;
extern crate libc;
#[macro_use] extern crate log;

use std::collections::HashSet;
use std::default::Default;
use std::io::{Write, self};

use syntax::ast;
use syntax::codemap::{DUMMY_SP, Span};
use syntax::print::{pp, pprust};
use syntax::print::pp::eof;
use syntax::ptr::P;
use rustc::plugin::Registry;

use types::Global;

mod types;
mod clangll;
mod clang;
mod gen;
mod parser;
mod bgmacro;

#[doc(hidden)]
#[plugin_registrar]
pub fn plugin_registrar(reg: &mut Registry) {
    reg.register_macro("bindgen", bgmacro::bindgen_macro);
}

pub struct BindgenOptions {
    pub match_pat: Vec<String>,
    pub builtins: bool,
    pub links: Vec<(String, LinkType)>,
    pub emit_ast: bool,
    pub fail_on_unknown_type: bool,
    pub override_enum_ty: String,
    pub clang_args: Vec<String>,
}

impl Default for BindgenOptions {
    fn default() -> BindgenOptions {
        BindgenOptions {
            match_pat: Vec::new(),
            builtins: false,
            links: Vec::new(),
            emit_ast: false,
            fail_on_unknown_type: false,
            override_enum_ty: "".to_string(),
            clang_args: Vec::new()
        }
    }
}

#[derive(Copy)]
pub enum LinkType {
    Default,
    Static,
    Framework
}

pub trait Logger {
    fn error(&self, msg: &str);
    fn warn(&self, msg: &str);
}

pub struct Bindings
{
    module: ast::Mod
}

impl Bindings
{
    pub fn generate(options: &BindgenOptions, logger: Option<&Logger>, span: Option<Span>) -> Result<Bindings, ()> {
        let l = DummyLogger;
        let logger = match logger {
            Some(l) => l,
            None => &l as &Logger
        };

        let span = match span {
            Some(s) => s,
            None => DUMMY_SP
        };

        let globals = try!(parse_headers(options, logger));

        let module = ast::Mod {
            inner: span,
            items: gen::gen_mod(&options.links[..], globals, span)
        };

        Ok(Bindings {
            module: module
        })
    }

    pub fn into_ast(self) -> Vec<P<ast::Item>> {
        self.module.items
    }

    pub fn to_string(&self) -> String {
        pprust::to_string(|s| {
            s.s = pp::mk_printer(Box::new(Vec::new()), 80);

            try!(s.print_mod(&self.module, &[]));
            s.print_remaining_comments()
        })
    }

    pub fn write(&self, writer: &mut (Write + 'static)) -> io::Result<()> {
        try!(writer.write("/* automatically generated by rust-bindgen */\n\n".as_bytes()));

        // This is safe as the Box<Writer> does not outlive ps or this function
        // Without this the interface is quite nasty
        let writer = unsafe { ::std::mem::transmute(writer) };
        let mut ps = pprust::rust_printer(writer);
        try!(ps.print_mod(&self.module, &[]));
        try!(ps.print_remaining_comments());
        try!(eof(&mut ps.s));
        ps.s.out.flush()
    }
}


struct DummyLogger;

impl Logger for DummyLogger {
    fn error(&self, _msg: &str) { }
    fn warn(&self, _msg: &str) { }
}

fn parse_headers(options: &BindgenOptions, logger: &Logger) -> Result<Vec<Global>, ()> {
    fn str_to_ikind(s: &str) -> Option<types::IKind> {
        match s {
            "uchar"     => Some(types::IUChar),
            "schar"     => Some(types::ISChar),
            "ushort"    => Some(types::IUShort),
            "sshort"    => Some(types::IShort),
            "uint"      => Some(types::IUInt),
            "sint"      => Some(types::IInt),
            "ulong"     => Some(types::IULong),
            "slong"     => Some(types::ILong),
            "ulonglong" => Some(types::IULongLong),
            "slonglong" => Some(types::ILongLong),
            _           => None,
        }
    }

    let clang_opts = parser::ClangParserOptions {
        builtin_names: builtin_names(),
        builtins: options.builtins,
        match_pat: options.match_pat.clone(),
        emit_ast: options.emit_ast,
        fail_on_unknown_type: options.fail_on_unknown_type,
        override_enum_ty: str_to_ikind(&options.override_enum_ty[..]),
        clang_args: options.clang_args.clone(),
    };

    parser::parse(clang_opts, logger)
}

fn builtin_names() -> HashSet<String> {
    let mut names = HashSet::new();
    let keys = [
        "__va_list_tag",
        "__va_list",
        "__builtin_va_list",
    ];

    keys.iter().all(|s| {
        names.insert(s.to_string());
        true
    });

    return names;
}
