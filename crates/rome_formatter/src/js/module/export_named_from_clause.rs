use crate::formatter_traits::{FormatOptionalTokenAndNode, FormatTokenAndNode};

use crate::{
    format_elements, space_token, token, FormatElement, FormatResult, Formatter, ToFormatElement,
};

use rslint_parser::ast::JsExportNamedFromClause;
use rslint_parser::ast::JsExportNamedFromClauseFields;

impl ToFormatElement for JsExportNamedFromClause {
    fn to_format_element(&self, formatter: &Formatter) -> FormatResult<FormatElement> {
        let JsExportNamedFromClauseFields {
            type_token,
            l_curly_token,
            specifiers,
            r_curly_token,
            from_token,
            source,
            assertion,
            semicolon_token,
        } = self.as_fields();

        let type_token = type_token
            .format_with_or_empty(formatter, |token| format_elements![token, space_token()])?;

        let specifiers = specifiers.format(formatter)?;

        let list = formatter.format_delimited_soft_block_spaces(
            &l_curly_token?,
            specifiers,
            &r_curly_token?,
        )?;

        let from = from_token.format(formatter)?;
        let source = source.format(formatter)?;
        let assertion = assertion.format_with_or_empty(formatter, |assertion| {
            format_elements![space_token(), assertion]
        })?;
        let semicolon = semicolon_token.format_or(formatter, || token(";"))?;

        Ok(format_elements![
            type_token,
            list,
            space_token(),
            from,
            space_token(),
            source,
            space_token(),
            assertion,
            semicolon
        ])
    }
}
