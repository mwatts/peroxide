use ast::SyntaxElement;
use environment::{Environment, RcEnv};
use std::cell::RefCell;
use std::rc::Rc;
use vm::Instruction;

/// This needs an RcEnv, not an &mut Env, because the Environment's count can get increased
/// through creating children environments.
pub fn compile(
    tree: &SyntaxElement,
    to: &mut Vec<Instruction>,
    env: RcEnv,
    tail: bool,
    toplevel: bool,
) -> Result<usize, String> {
    let initial_len = to.len();
    match tree {
        SyntaxElement::Quote(q) => {
            to.push(Instruction::Constant(q.quoted));
        }
        SyntaxElement::If(i) => {
            compile(&i.cond, to, env.clone(), false, false)?;
            let conditional_jump_idx = to.len();
            to.push(Instruction::NoOp); // Is rewritten as a conditional jump below
            compile(&i.t, to, env.clone(), tail, false)?;
            let mut true_end = to.len();
            if let Some(ref f) = i.f {
                to.push(Instruction::NoOp);
                true_end += 1;
                compile(f, to, env.clone(), tail, false)?;
                to[true_end - 1] = Instruction::Jump(to.len() - true_end);
            }
            to[conditional_jump_idx] = Instruction::JumpFalse(true_end - conditional_jump_idx - 1);
        }
        SyntaxElement::Begin(b) => {
            compile_sequence(&b.expressions, to, env.clone(), tail)?;
        }
        SyntaxElement::Set(s) => {
            if let Some((depth, index)) = env.borrow().get(&s.variable) {
                compile(&s.value, to, env.clone(), false, false)?;
                to.push(Instruction::DeepArgumentSet { depth, index });
            } else {
                return Err(format!("Undefined value {}.", &s.variable));
            }
        }
        SyntaxElement::Reference(r) => {
            if let Some((depth, index)) = env.borrow().get(&r.variable) {
                to.push(Instruction::DeepArgumentGet { depth, index });
            } else {
                return Err(format!("Undefined value {}.", &r.variable));
            }
        }
        SyntaxElement::Lambda(l) => {
            if l.formals.rest.is_some() {
                return Err("Only fixed functions can be compiled for now.".into());
            }
            to.push(Instruction::CreateClosure(1));
            let skip_pos = to.len();
            to.push(Instruction::NoOp); // Skip over function definition
            to.push(Instruction::CheckArity {
                arity: l.formals.values.len(),
                dotted: false,
            });
            to.push(Instruction::ExtendEnv);
            let formal_name_refs = l
                .formals
                .values
                .iter()
                .map(std::ops::Deref::deref)
                .collect::<Vec<_>>();
            let lambda_env = Rc::new(RefCell::new(Environment::new_initial(
                Some(env.clone()),
                &formal_name_refs,
            )));
            compile_sequence(&l.expressions, to, lambda_env.clone(), true)?;
            to.push(Instruction::Return);
            to[skip_pos] = Instruction::Jump(to.len() - skip_pos - 1);
        }
        SyntaxElement::Define(d) => {
            if toplevel {
                // TODO refactor this to share code with set!
                // The prt here doesn't sound super useful but it makes sure the borrow doesn't
                // live into the `else` block, where it would conflict with the borrow_mut.
                // Some((usize, usize)) is a Copy type, so the borrow can be dropped.

                let ptr = env.borrow().get(&d.variable);
                if let Some((depth, index)) = ptr {
                    compile(&d.value, to, env.clone(), false, false)?;
                    to.push(Instruction::DeepArgumentSet { depth, index });
                } else {
                    env.borrow_mut().define(&d.variable);
                    compile(&d.value, to, env.clone(), false, false)?;
                    to.push(Instruction::ExtendFrame);
                }
            } else {
                return Err("Non-top-level defines not yet supported.".into());
            }
        }
        SyntaxElement::Application(a) => {
            compile(&a.function, to, env.clone(), false, false)?;
            to.push(Instruction::PushValue);
            for instr in a.args.iter() {
                compile(instr, to, env.clone(), false, false)?;
                to.push(Instruction::PushValue);
            }
            to.push(Instruction::CreateFrame(a.args.len()));
            to.push(Instruction::PopFunction);
            to.push(Instruction::PreserveEnv);
            to.push(Instruction::FunctionInvoke);
            to.push(Instruction::RestoreEnv);
        }
    }
    Ok(to.len() - initial_len)
}

fn compile_sequence(
    expressions: &[SyntaxElement],
    to: &mut Vec<Instruction>,
    env: RcEnv,
    tail: bool,
) -> Result<usize, String> {
    let initial_len = to.len();
    for instr in expressions[..expressions.len() - 1].iter() {
        compile(instr, to, env.clone(), false, false)?;
    }
    compile(
        // This should have been caught at the syntax step.
        expressions.last().expect("Empty sequence."),
        to,
        env.clone(),
        tail,
        false,
    )?;
    Ok(to.len() - initial_len)
}