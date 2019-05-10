// Copyright 2018-2019 Matthieu Felix
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
// https://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

use ast::SyntaxElement;
use environment::{Environment, EnvironmentValue, RcEnv, Variable};
use std::cell::RefCell;
use std::rc::Rc;
use vm::Instruction;

// This needs an RcEnv, not an &mut Env, because the Environment's count can get increased
// through creating children environments.
pub fn compile(
    tree: &SyntaxElement,
    to: &mut Vec<Instruction>,
    env: RcEnv,
    tail: bool,
    toplevel: bool,
) -> Result<usize, String> {
    if tail && toplevel {
        panic!("Toplevel expression is not in tail position")
    }
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
            if let Some(EnvironmentValue::Variable(v)) = env.borrow().get(&s.variable) {
                compile(&s.value, to, env.clone(), false, false)?;
                to.push(make_set_instruction(&env, &v));
            } else {
                return Err(format!("Undefined value {}.", &s.variable));
            }
        }
        SyntaxElement::Reference(r) => {
            if let Some(EnvironmentValue::Variable(v)) = env.borrow().get(&r.variable) {
                to.push(make_get_instruction(&env, &v));
            } else {
                return Err(format!("Undefined value {}.", &r.variable));
            }
        }
        SyntaxElement::Lambda(l) => {
            let arity = l.formals.values.len();
            let dotted = l.formals.rest.is_some();

            to.push(Instruction::CreateClosure(1));
            let skip_pos = to.len();
            to.push(Instruction::NoOp); // Will be replaced with over function code
            to.push(Instruction::CheckArity { arity, dotted });
            if dotted {
                to.push(Instruction::PackFrame(arity));
            }
            to.push(Instruction::ExtendEnv);

            let mut formal_name_refs: Vec<_> = l
                .formals
                .values
                .iter()
                .map(std::ops::Deref::deref)
                .collect();
            if let Some(ref rest) = l.formals.rest {
                formal_name_refs.push(rest);
            }
            let lambda_env = Rc::new(RefCell::new(Environment::new_initial(
                Some(env.clone()),
                &formal_name_refs,
            )));

            compile_sequence(&l.expressions, to, lambda_env.clone(), true)?;
            to.push(Instruction::Return);
            to[skip_pos] = Instruction::Jump(to.len() - skip_pos - 1);
        }
        SyntaxElement::Define(d) => {
            // TODO all top-level defines should actually cause a frame extension immediately,
            //      not after the expression is evaluated. There is currently a bug where a failing
            //      define will permanently make env and af out of sync.
            if toplevel {
                // TODO refactor this to share code with set!
                // The prt here doesn't sound super useful but it makes sure the borrow doesn't
                // live into the `else` block, where it would conflict with the borrow_mut.

                let ptr = env.borrow().get(&d.variable);
                if let Some(EnvironmentValue::Variable(v)) = ptr {
                    compile(&d.value, to, env.clone(), false, false)?;
                    to.push(make_set_instruction(&env, &v));
                } else {
                    env.borrow_mut().define(&d.variable, true);
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
            if !tail {
                to.push(Instruction::PreserveEnv);
            }
            to.push(Instruction::FunctionInvoke { tail });
            if !tail {
                to.push(Instruction::RestoreEnv);
            }
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

fn make_get_instruction(env: &RcEnv, variable: &Variable) -> Instruction {
    let depth = env.borrow().depth(variable.altitude);
    let index = variable.index;
    match (variable.altitude, variable.initialized) {
        (0, true) => Instruction::GlobalArgumentGet { index },
        (0, false) => Instruction::CheckedGlobalArgumentGet { index },
        (_, true) => Instruction::LocalArgumentGet { depth, index },
        (_, false) => Instruction::CheckedLocalArgumentGet { depth, index },
    }
}

fn make_set_instruction(env: &RcEnv, variable: &Variable) -> Instruction {
    let depth = env.borrow().depth(variable.altitude);
    let index = variable.index;
    match variable.altitude {
        0 => Instruction::GlobalArgumentSet { index },
        _ => Instruction::DeepArgumentSet { depth, index },
    }
}
