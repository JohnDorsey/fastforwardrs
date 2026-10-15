//! S2: a guard that lives in the rewriting step: every loop of a shader gets a counter of its own and leaves after `cap` iterations (naga IR level, so it works for
//! shaders that came from GLSL and from WGSL alike). Throwaway spike code.
use naga::{
    BinaryOperator, Block, Expression, Function, Handle, Literal, LocalVariable, Module, Range, Scalar, Span,
    Statement, Type, TypeInner,
};

pub fn cap_loops(module: &mut Module, cap: u32) -> usize {
    let u32_type = module.types.insert(
        Type {
            name: None,
            inner: TypeInner::Scalar(Scalar::U32),
        },
        Span::UNDEFINED,
    );
    let mut capped = 0;
    for (_, function) in module.functions.iter_mut() {
        capped += cap_in_function(function, u32_type, cap);
    }
    for entry_point in module.entry_points.iter_mut() {
        capped += cap_in_function(&mut entry_point.function, u32_type, cap);
    }
    capped
}

fn cap_in_function(function: &mut Function, u32_type: Handle<Type>, cap: u32) -> usize {
    let mut body = std::mem::take(&mut function.body);
    let capped = cap_in_block(&mut body, function, u32_type, cap);
    function.body = body;
    capped
}

fn cap_in_block(block: &mut Block, function: &mut Function, u32_type: Handle<Type>, cap: u32) -> usize {
    let mut capped = 0;
    for (statement, _) in block.span_iter_mut() {
        match statement {
            Statement::Block(inner) => capped += cap_in_block(inner, function, u32_type, cap),
            Statement::If { accept, reject, .. } => {
                capped += cap_in_block(accept, function, u32_type, cap);
                capped += cap_in_block(reject, function, u32_type, cap);
            }
            Statement::Switch { cases, .. } => {
                for case in cases.iter_mut() {
                    capped += cap_in_block(&mut case.body, function, u32_type, cap);
                }
            }
            Statement::Loop { body, continuing, .. } => {
                capped += cap_in_block(body, function, u32_type, cap);
                capped += cap_in_block(continuing, function, u32_type, cap);
                // A counter that begins at 0 when the shader begins, counts every pass through THIS loop (also over several entries into it), and breaks at the cap.
                let zero = function
                    .expressions
                    .append(Expression::Literal(Literal::U32(0)), Span::UNDEFINED);
                let counter = function.local_variables.append(
                    LocalVariable {
                        name: Some("_isf_loop_counter".to_string()),
                        ty: u32_type,
                        init: Some(zero),
                    },
                    Span::UNDEFINED,
                );
                let pointer = function
                    .expressions
                    .append(Expression::LocalVariable(counter), Span::UNDEFINED);
                let one = function
                    .expressions
                    .append(Expression::Literal(Literal::U32(1)), Span::UNDEFINED);
                let limit = function
                    .expressions
                    .append(Expression::Literal(Literal::U32(cap)), Span::UNDEFINED);
                let loaded = function
                    .expressions
                    .append(Expression::Load { pointer }, Span::UNDEFINED);
                let added = function.expressions.append(
                    Expression::Binary {
                        op: BinaryOperator::Add,
                        left: loaded,
                        right: one,
                    },
                    Span::UNDEFINED,
                );
                let too_many = function.expressions.append(
                    Expression::Binary {
                        op: BinaryOperator::Greater,
                        left: added,
                        right: limit,
                    },
                    Span::UNDEFINED,
                );
                let mut prefix = Block::new();
                prefix.push(
                    Statement::Emit(Range::new_from_bounds(loaded, too_many)),
                    Span::UNDEFINED,
                );
                prefix.push(Statement::Store { pointer, value: added }, Span::UNDEFINED);
                let mut leave = Block::new();
                leave.push(Statement::Break, Span::UNDEFINED);
                prefix.push(
                    Statement::If {
                        condition: too_many,
                        accept: leave,
                        reject: Block::new(),
                    },
                    Span::UNDEFINED,
                );
                prefix.extend_block(std::mem::take(body));
                *body = prefix;
                capped += 1;
            }
            _ => {}
        }
    }
    capped
}
