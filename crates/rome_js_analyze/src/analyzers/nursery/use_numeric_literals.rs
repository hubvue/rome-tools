use crate::semantic_services::Semantic;
use crate::{ast_utils, JsRuleAction};
use rome_analyze::context::RuleContext;
use rome_analyze::{declare_rule, ActionCategory, Rule, RuleDiagnostic};
use rome_console::markup;
use rome_diagnostics::Applicability;
use rome_js_factory::make;
use rome_js_semantic::SemanticModel;
use rome_js_syntax::{
    JsAnyExpression, JsAnyLiteralExpression, JsAnyMemberExpression, JsCallExpression, JsSyntaxToken,
};
use rome_rowan::{AstNode, AstSeparatedList, BatchMutationExt};

declare_rule! {
    /// Disallow `parseInt()` and `Number.parseInt()` in favor of binary, octal, and hexadecimal literals
    ///
    /// ## Examples
    ///
    /// ### Invalid
    ///
    /// ```js,expect_diagnostic
    /// parseInt("111110111", 2) === 503;
    /// ```
    ///
    /// ```js,expect_diagnostic
    /// Number.parseInt("767", 8) === 503;
    /// ```
    /// ### Valid
    ///
    /// ```js
    /// parseInt(1);
    /// parseInt(1, 3);
    /// Number.parseInt(1);
    /// Number.parseInt(1, 3);
    ///
    /// 0b111110111 === 503;
    /// 0o767 === 503;
    /// 0x1F7 === 503;
    ///
    /// a[parseInt](1,2);
    ///
    /// parseInt(foo);
    /// parseInt(foo, 2);
    /// Number.parseInt(foo);
    /// Number.parseInt(foo, 2);
    /// ```
    pub(crate) UseNumericLiterals {
        version: "11.0.0",
        name: "useNumericLiterals",
        recommended: false,
    }
}

pub struct CallInfo {
    callee: &'static str,
    text: String,
    radix: Radix,
}

impl Rule for UseNumericLiterals {
    type Query = Semantic<JsCallExpression>;
    type State = CallInfo;
    type Signals = Option<Self::State>;
    type Options = ();

    fn run(ctx: &RuleContext<Self>) -> Option<Self::State> {
        let expr = ctx.query();
        let model = ctx.model();
        CallInfo::try_from_expr(expr, model)
    }

    fn diagnostic(ctx: &RuleContext<Self>, state: &Self::State) -> Option<RuleDiagnostic> {
        let node = ctx.query();

        Some(RuleDiagnostic::new(
            rule_category!(),
            node.range(),
            markup! { "Use "{state.radix.description()}" literals instead of "{state.callee} }
                .to_owned(),
        ))
    }

    fn action(ctx: &RuleContext<Self>, call: &Self::State) -> Option<JsRuleAction> {
        let node = ctx.query();
        let mut mutation = ctx.root().begin();

        let number = call.to_numeric_literal()?;
        let number = ast_utils::token_with_source_trivia(number, node);

        mutation.replace_node_discard_trivia(
            JsAnyExpression::JsCallExpression(node.clone()),
            JsAnyExpression::JsAnyLiteralExpression(
                JsAnyLiteralExpression::JsNumberLiteralExpression(
                    make::js_number_literal_expression(number),
                ),
            ),
        );

        Some(JsRuleAction {
            category: ActionCategory::QuickFix,
            applicability: Applicability::MaybeIncorrect,
            message: markup! { "Replace with "{call.radix.description()}" literals" }.to_owned(),
            mutation,
        })
    }
}

impl CallInfo {
    fn try_from_expr(expr: &JsCallExpression, model: &SemanticModel) -> Option<CallInfo> {
        let args = expr.arguments().ok()?.args();
        if args.len() != 2 {
            return None;
        }
        let mut args = args.into_iter();
        let text = args
            .next()?
            .ok()?
            .as_js_any_expression()?
            .as_string_constant()?;
        let radix = args
            .next()?
            .ok()?
            .as_js_any_expression()?
            .as_js_any_literal_expression()?
            .as_js_number_literal_expression()?
            .as_number()?;
        let callee = get_callee(expr, model)?;

        Some(CallInfo {
            callee,
            text,
            radix: Radix::from_f64(radix)?,
        })
    }

    fn to_numeric_literal(&self) -> Option<JsSyntaxToken> {
        i128::from_str_radix(&self.text, self.radix as u32).ok()?;
        let number = format!("{}{}", self.radix.prefix(), self.text);
        let number = make::js_number_literal(&number);
        Some(number)
    }
}

fn get_callee(expr: &JsCallExpression, model: &SemanticModel) -> Option<&'static str> {
    let callee = expr.callee().ok()?.omit_parentheses();
    if let Some(id) = callee.as_reference_identifier() {
        if id.has_name("parseInt") && model.declaration(&id).is_none() {
            return Some("parseInt()");
        }
    }

    let callee = JsAnyMemberExpression::cast_ref(callee.syntax())?;
    let object = callee.get_object_reference_identifier()?;
    if object.has_name("Number")
        && model.declaration(&object).is_none()
        && callee.has_member_name("parseInt")
    {
        return Some("Number.parseInt()");
    }

    None
}

#[derive(Copy, Clone)]
enum Radix {
    Binary = 2,
    Octal = 8,
    Hexadecimal = 16,
}

impl Radix {
    fn from_f64(v: f64) -> Option<Self> {
        Some(if v == 2.0 {
            Self::Binary
        } else if v == 8.0 {
            Self::Octal
        } else if v == 16.0 {
            Self::Hexadecimal
        } else {
            return None;
        })
    }

    fn prefix(&self) -> &'static str {
        match self {
            Radix::Binary => "0b",
            Radix::Octal => "0o",
            Radix::Hexadecimal => "0x",
        }
    }

    fn description(&self) -> &'static str {
        match self {
            Radix::Binary => "binary",
            Radix::Octal => "octal",
            Radix::Hexadecimal => "hexadecimal",
        }
    }
}
