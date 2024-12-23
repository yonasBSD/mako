use std::collections::HashMap;

use indexmap::IndexMap;
use swc_core::common::{Mark, SyntaxContext, DUMMY_SP};
use swc_core::ecma::ast::{Expr, Ident, MemberExpr, Module, ModuleItem, VarDeclKind};
use swc_core::ecma::utils::{quote_ident, quote_str, ExprFactory};
use swc_core::ecma::visit::{VisitMut, VisitMutWith};

use crate::ast::DUMMY_CTXT;
use crate::config::Providers;

pub struct Provide {
    unresolved_mark: Mark,
    top_level_mark: Mark,
    providers: Providers,
    var_decls: IndexMap<String, ModuleItem>,
}

impl Provide {
    pub fn new(providers: Providers, unresolved_mark: Mark, top_level_mark: Mark) -> Self {
        Self {
            unresolved_mark,
            top_level_mark,
            providers,
            var_decls: Default::default(),
        }
    }
}

impl VisitMut for Provide {
    fn visit_mut_module(&mut self, module: &mut Module) {
        module.visit_mut_children_with(self);
        module
            .body
            .splice(0..0, self.var_decls.iter().map(|(_, var)| var.clone()));
        module.visit_mut_with(&mut ToTopLevelVars::new(
            self.unresolved_mark,
            self.top_level_mark,
            &self.var_decls,
        ))
    }
    fn visit_mut_ident(&mut self, n: &mut Ident) {
        let has_binding = n.ctxt.outer() != self.unresolved_mark;
        let name = &n.sym.to_string();
        let provider = self.providers.get(name);

        if !has_binding && provider.is_some() {
            if let Some((from, key)) = provider {
                let require_decl: ModuleItem = {
                    if key.is_empty() {
                        // eg: const process = require('process');
                        quote_ident!("__mako_require__")
                            .as_call(DUMMY_SP, vec![quote_str!(from.as_str()).as_arg()])
                            .into_var_decl(VarDeclKind::Const, n.clone().into())
                            .into()
                    } else {
                        // require("buffer")
                        let require_expr = quote_ident!("__mako_require__")
                            .as_call(DUMMY_SP, vec![quote_str!(from.as_str()).as_arg()]);

                        // eg const Buffer = require("buffer").Buffer;
                        Expr::Member(MemberExpr {
                            obj: require_expr.into(),
                            span: DUMMY_SP,
                            prop: quote_ident!(key.as_str()).into(),
                        })
                        .into_var_decl(VarDeclKind::Const, n.clone().into())
                        .into()
                    }
                };

                self.var_decls.insert(name.to_string(), require_decl);
            }
        }
    }
}

struct ToTopLevelVars {
    unresolved_mark: Mark,
    replaces_map: HashMap<String, SyntaxContext>,
}

impl ToTopLevelVars {
    fn new(
        unresolved_mark: Mark,
        top_level_mark: Mark,
        vars: &IndexMap<String, ModuleItem>,
    ) -> Self {
        let mut replaces: HashMap<String, SyntaxContext> = Default::default();

        vars.iter().for_each(|(k, _)| {
            let ctxt = DUMMY_CTXT.apply_mark(top_level_mark);
            replaces.insert(k.clone(), ctxt);
        });

        Self {
            unresolved_mark,
            replaces_map: replaces,
        }
    }
}

impl VisitMut for ToTopLevelVars {
    fn visit_mut_ident(&mut self, i: &mut Ident) {
        if i.ctxt.outer() == self.unresolved_mark {
            if let Some(ctxt) = self.replaces_map.get(&i.sym.to_string()) {
                i.ctxt = *ctxt;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use swc_core::common::GLOBALS;
    use swc_core::ecma::visit::VisitMutWith;

    use super::Provide;
    use crate::ast::tests::TestUtils;

    #[test]
    fn test_provide_normal() {
        assert_eq!(
            run(r#"
console.log(process);
console.log(process.env);
Buffer.from('foo');
function foo() {
    let process = 1;
    console.log(process);
    let Buffer = 'b';
    Buffer.from('foo');
}
            "#),
            r#"
const process = __mako_require__("process");
const Buffer = __mako_require__("buffer").Buffer;
console.log(process);
console.log(process.env);
Buffer.from('foo');
function foo() {
    let process = 1;
    console.log(process);
    let Buffer = 'b';
    Buffer.from('foo');
}
            "#
            .trim()
        );
    }

    #[test]
    fn test_provide_in_shorthand_notation() {
        assert_eq!(
            run(r#"
console.log({process});
            "#),
            r#"
const process = __mako_require__("process");
console.log({
    process
});
            "#
            .trim()
        );
    }

    fn run(js_code: &str) -> String {
        let mut test_utils = TestUtils::gen_js_ast(js_code);
        let ast = test_utils.ast.js_mut();
        GLOBALS.set(&test_utils.context.meta.script.globals, || {
            let mut providers = HashMap::new();
            providers.insert("process".into(), ("process".into(), "".into()));
            providers.insert("Buffer".into(), ("buffer".into(), "Buffer".into()));
            let mut visitor = Provide::new(providers, ast.unresolved_mark, ast.top_level_mark);
            ast.ast.visit_mut_with(&mut visitor);
        });
        test_utils.js_ast_to_code()
    }
}
