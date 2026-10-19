//! Syntax-only source reading for canonical formatting. No assembly state is
//! consulted: `.end` is a statement, not permission to stop reading the file.

use crate::ast::{Comment, Item, Node, Program, Scope, Symbol, Trivia};
use crate::span::Span;

pub(super) fn parse(source: &str) -> Program {
    let mut nodes = Vec::new();
    let mut leading = Vec::new();
    for (index, raw) in source.lines().enumerate() {
        let span = Span::at(index as u32 + 1, 1);
        let (code, comment) = super::split_comment(raw);
        let code = code.trim();
        if code.is_empty() {
            leading.push(Comment {
                text: comment.unwrap_or("").to_string(),
                span,
            });
            continue;
        }
        let trivia = Trivia {
            leading: std::mem::take(&mut leading),
            trailing: comment.map(|text| Comment {
                text: text.to_string(),
                span: span.clone(),
            }),
        };
        let (label, item, text) = if let Some(eq) = super::assignment_split(code) {
            (
                Some(symbol(code[..eq].trim())),
                Some(Item::SourceAssignment),
                format!("= {}", code[eq + 1..].trim_start()),
            )
        } else if super::end_token_len(code).is_none()
            && let Some((head, rest)) = super::split_label_head(code)
        {
            (
                Some(symbol(&head[..head.len() - 1])),
                None,
                rest.to_string(),
            )
        } else {
            (None, None, code.to_string())
        };
        nodes.push(Node {
            operand_span: None,
            label,
            item,
            source: text,
            span,
            trivia,
        });
    }
    if !leading.is_empty() {
        nodes.push(Node {
            operand_span: None,
            label: None,
            item: None,
            source: String::new(),
            span: Span::at(source.lines().count() as u32, 1),
            trivia: Trivia {
                leading,
                trailing: None,
            },
        });
    }
    Program { nodes }
}

fn symbol(name: &str) -> Symbol {
    Symbol {
        name: name.to_string(),
        qualified: name.to_string(),
        scope: Scope::Global,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unread_assignments_cannot_enter_the_engine() {
        let program = parse("IGNORED = $100000000\n");
        let item = program.nodes[0].item.as_ref().expect("source assignment");
        assert!(crate::ast::lower_item_ref(item).is_err());
        assert!(crate::ast::lower(program).is_err());
    }
}
