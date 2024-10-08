use std::sync::Arc;

use swc_core::common::GLOBALS;
use swc_core::ecma::transforms::base::hygiene::{self, hygiene_with_config};
use swc_core::ecma::transforms::base::resolver;
use swc_core::ecma::visit::VisitMutWith;

use super::css_ast::{CSSAstGenerated, CssAst};
use super::file::{Content, File, JsContent};
use super::js_ast::{JSAstGenerated, JsAst};
use crate::compiler::Context;
use crate::config::Mode;

pub struct TestUtilsOpts {
    pub file: Option<String>,
    pub content: Option<String>,
}

#[derive(Debug)]
pub enum TestAst {
    Js(JsAst),
    Css(CssAst),
}

impl TestAst {
    pub fn js(&self) -> &JsAst {
        match self {
            TestAst::Js(ast) => ast,
            _ => panic!("Not a js ast"),
        }
    }

    pub fn css_mut(&mut self) -> &mut CssAst {
        match self {
            TestAst::Css(ast) => ast,
            _ => panic!("Not a css ast"),
        }
    }
    pub fn js_mut(&mut self) -> &mut JsAst {
        match self {
            TestAst::Js(ast) => ast,
            _ => panic!("Not a js ast"),
        }
    }
}

pub struct TestUtils {
    pub ast: TestAst,
    pub context: Arc<Context>,
}

impl TestUtils {
    pub fn new(opts: TestUtilsOpts) -> Self {
        let mut context = Context {
            ..Default::default()
        };
        context.config.devtool = None;
        let context = Arc::new(context);
        TestUtils::with_context(opts, context)
    }

    pub fn with_mode_production(opts: TestUtilsOpts) -> Self {
        let mut context = Context {
            ..Default::default()
        };
        context.config.devtool = None;
        context.config.mode = Mode::Production;
        let context = Arc::new(context);
        TestUtils::with_context(opts, context)
    }

    pub fn with_context(opts: TestUtilsOpts, context: Arc<Context>) -> Self {
        let file = if let Some(file) = opts.file {
            file
        } else {
            "test.js".to_string()
        };
        let is_jsx = file.ends_with(".jsx") || file.ends_with(".tsx");
        let mut file = File::new(file, context.clone());
        let is_css = file.extname == "css";
        let content = if let Some(content) = opts.content {
            content
        } else {
            "".to_string()
        };
        if is_css {
            file.set_content(Content::Css(content));
        } else {
            file.set_content(Content::Js(JsContent { content, is_jsx }));
        }
        let ast = if is_css {
            TestAst::Css(CssAst::new(&file, context.clone(), false).unwrap())
        } else {
            TestAst::Js(JsAst::new(&file, context.clone()).unwrap())
        };
        Self { ast, context }
    }

    pub fn gen_css_ast(content: String, is_prod: bool) -> Self {
        let opts = TestUtilsOpts {
            file: Some("test.css".to_string()),
            content: Some(content),
        };
        if is_prod {
            Self::with_mode_production(opts)
        } else {
            Self::new(opts)
        }
    }

    pub fn gen_js_ast<T: AsRef<str>>(content: T) -> Self {
        let c = content.as_ref().to_string();
        let mut test_utils = Self::new(TestUtilsOpts {
            file: Some("test.js".to_string()),
            content: Some(c),
        });
        let ast = test_utils.ast.js_mut();
        let unresolved_mark = ast.unresolved_mark;
        let top_level_mark = ast.top_level_mark;
        GLOBALS.set(&test_utils.context.meta.script.globals, || {
            ast.ast
                .visit_mut_with(&mut resolver(unresolved_mark, top_level_mark, false));
        });
        test_utils
    }

    pub fn js_ast_to_code(&mut self) -> String {
        let ast = self.ast.js_mut();
        let top_level_mark = ast.top_level_mark;
        GLOBALS.set(&self.context.meta.script.globals, || {
            ast.ast
                .visit_mut_with(&mut hygiene_with_config(hygiene::Config {
                    top_level_mark,
                    ..Default::default()
                }));
        });
        let JSAstGenerated { code, sourcemap: _ } = ast.generate(self.context.clone()).unwrap();
        code.trim_end().to_string()
    }

    pub fn css_ast_to_code(&mut self) -> String {
        let ast = self.ast.css_mut();
        let CSSAstGenerated { code, sourcemap: _ } = ast.generate(self.context.clone()).unwrap();
        code.trim_end().to_string()
    }
}
