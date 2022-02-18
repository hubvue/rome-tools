use crate::formatter_traits::FormatTokenAndNode;

use crate::{FormatElement, FormatResult, Formatter, ToFormatElement};

use rslint_parser::ast::JsCatchDeclaration;
use rslint_parser::ast::JsCatchDeclarationFields;

impl ToFormatElement for JsCatchDeclaration {
    fn to_format_element(&self, formatter: &Formatter) -> FormatResult<FormatElement> {
        let JsCatchDeclarationFields {
            l_paren_token,
            binding,
            r_paren_token,
        } = self.as_fields();

        formatter.format_delimited_soft_block_indent(
            &l_paren_token?,
            binding.format(formatter)?,
            &r_paren_token?,
        )
    }
}
