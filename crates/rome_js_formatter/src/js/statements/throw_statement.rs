use crate::prelude::*;

use crate::js::statements::return_statement::JsAnyStatementWithArgument;
use rome_js_syntax::JsThrowStatement;

#[derive(Debug, Clone, Default)]
pub struct FormatJsThrowStatement;

impl FormatNodeRule<JsThrowStatement> for FormatJsThrowStatement {
    fn fmt_fields(&self, node: &JsThrowStatement, f: &mut JsFormatter) -> FormatResult<()> {
        JsAnyStatementWithArgument::from(node.clone()).fmt(f)
    }

    fn fmt_dangling_comments(&self, _: &JsThrowStatement, _: &mut JsFormatter) -> FormatResult<()> {
        // Formatted inside of `JsAnyStatementWithArgument`
        Ok(())
    }
}
