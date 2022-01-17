use super::{state::VmState, Variable, VmArgumentList};
use crate::{vm::argument_list::VmArgument, Context, ExpressionError, Function, Value};
use diagnostic::Span;
use std::{collections::BTreeMap, ops::Deref};

#[derive(Copy, Clone, Debug, PartialEq, Eq)]
pub enum OpCode {
    Abort,
    Return,
    Constant,
    Negate,
    Add,
    Subtract,
    Multiply,
    Divide,
    Rem,
    Merge,
    Not,
    Greater,
    GreaterEqual,
    Less,
    LessEqual,
    NotEqual,
    Equal,
    Pop,
    ClearError,
    JumpIfFalse,
    JumpIfTrue,
    JumpIfNotErr,
    Jump,
    SetPathInfallible,
    SetPath,
    GetPath,
    Call,
    CreateArray,
    CreateObject,
    EmptyParameter,
    MoveParameter,
    MoveStatic,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum Instruction {
    OpCode(OpCode),
    Primitive(usize),
}

#[derive(Debug, Default)]
pub struct Vm {
    fns: Vec<Box<dyn Function + Send + Sync>>,
    pub(super) instructions: Vec<Instruction>,
    pub(super) values: Vec<Value>,
    targets: Vec<Variable>,
    static_params: Vec<Box<dyn std::any::Any + Send + Sync>>,
}

impl Vm {
    pub fn new(fns: Vec<Box<dyn Function + Send + Sync>>) -> Self {
        Self {
            fns,
            ..Default::default()
        }
    }

    pub fn add_constant(&mut self, object: Value) -> usize {
        self.values.push(object);
        self.values.len() - 1
    }

    pub fn write_chunk(&mut self, code: OpCode) {
        self.instructions.push(Instruction::OpCode(code));
    }

    pub fn write_chunk_at(&mut self, pos: usize, code: OpCode) {
        self.instructions[pos] = Instruction::OpCode(code);
    }

    pub fn instructions(&self) -> &Vec<Instruction> {
        &self.instructions
    }

    pub fn write_primitive(&mut self, code: usize) {
        self.instructions.push(Instruction::Primitive(code));
    }

    pub fn write_primitive_at(&mut self, pos: usize, code: usize) {
        self.instructions[pos] = Instruction::Primitive(code);
    }

    pub fn function(&self, function_id: usize) -> Option<&(dyn Function + Send + Sync)> {
        self.fns.get(function_id).map(|fun| fun.deref())
    }

    /// Gets a target from the list of targets used, if it hasn't already been added then add it.
    pub fn get_target(&mut self, target: &Variable) -> usize {
        match self.targets.iter().position(|t| t == target) {
            Some(pos) => pos,
            None => {
                self.targets.push(target.clone());
                self.targets.len() - 1
            }
        }
    }

    /// Adds a static argument to the list and returns the position of this in the list.
    pub fn add_static(&mut self, stat: Box<dyn std::any::Any + Send + Sync>) -> usize {
        self.static_params.push(stat);
        self.static_params.len() - 1
    }

    /// For debugging purposes, returns a list of strings representing the instructions and primitives.
    pub fn disassemble(&self) -> Vec<String> {
        self.instructions
            .iter()
            .enumerate()
            .map(|(idx, inst)| match inst {
                Instruction::OpCode(opcode) => format!("{:04}: {:?}", idx, opcode),
                Instruction::Primitive(primitive) => format!("{:04}: {}", idx, primitive),
            })
            .collect()
    }

    pub fn emit_jump(&mut self, instruction: OpCode) -> usize {
        self.write_chunk(instruction);

        // Insert placeholder
        self.write_primitive(usize::MAX);

        self.instructions().len() - 1
    }

    /// When compiling an `if` statement we don't know initially where we want to jump to if the predicate is
    /// false.
    /// To work this, we initially jump to an arbitrary position. Then compile the ensuing block which will allow
    /// us to work out where we need to jump. We can then return to the initial jump and update it with the offset.
    pub fn patch_jump(&mut self, offset: usize) {
        let jump = self.instructions.len() - offset - 1;
        self.write_primitive_at(offset, jump);
    }

    /// Interpret the VM.
    /// Interpreting is essentially a process of looping through a list of intstructions and interpreting
    /// each one.
    /// The VM is stack based. When the `Return` OpCode is encountered the top item on the stack is popped and returned.
    /// It is expected that the final instruction is a `Return`.
    pub fn interpret<'a>(&self, ctx: &mut Context<'a>) -> Result<Value, ExpressionError> {
        // Any mutable state during the run is stored here.
        let mut state: VmState = VmState::new(self);

        loop {
            let next = state.next()?;

            match next {
                OpCode::Abort => {
                    // Aborts the process.
                    let start = state.next_primitive()?;
                    let end = state.next_primitive()?;
                    return Err(ExpressionError::Abort {
                        span: Span::new(start, end),
                    });
                }
                OpCode::Return => {
                    // Ends the process and returns the top item from the stack - or Null if the stack is empty.
                    return Ok(state.stack.pop().unwrap_or(Value::Null));
                }
                OpCode::Constant => {
                    let value = state.read_constant()?;
                    state.stack.push(value);
                }
                OpCode::Negate => match state.stack.pop() {
                    None => return Err("Negating nothing".into()),
                    Some(Value::Float(value)) => state.stack.push(Value::Float(value * -1.0)),
                    _ => return Err("Negating non number".into()),
                },
                OpCode::Not => match state.stack.pop() {
                    None => return Err("Notting nothing".into()),
                    Some(Value::Boolean(value)) => state.stack.push(Value::Boolean(!value)),
                    _ => return Err("Notting non boolean".into()),
                },
                OpCode::Add => binary_op(&mut state, Value::try_add)?,
                OpCode::Subtract => binary_op(&mut state, Value::try_sub)?,
                OpCode::Multiply => binary_op(&mut state, Value::try_mul)?,
                OpCode::Divide => binary_op(&mut state, Value::try_div)?,
                OpCode::Rem => binary_op(&mut state, Value::try_rem)?,
                OpCode::Merge => binary_op(&mut state, Value::try_merge)?,
                OpCode::Greater => binary_op(&mut state, Value::try_gt)?,
                OpCode::GreaterEqual => binary_op(&mut state, Value::try_ge)?,
                OpCode::Less => binary_op(&mut state, Value::try_lt)?,
                OpCode::LessEqual => binary_op(&mut state, Value::try_le)?,
                OpCode::NotEqual => {
                    let rhs = state.pop_stack()?;
                    let lhs = state.pop_stack()?;
                    state.stack.push((!lhs.eq_lossy(&rhs)).into());
                }
                OpCode::Equal => {
                    let rhs = state.pop_stack()?;
                    let lhs = state.pop_stack()?;
                    state.stack.push(lhs.eq_lossy(&rhs).into());
                }
                OpCode::Pop => {
                    // Removes the top item from the stack
                    let _ = state.stack.pop();
                }
                OpCode::ClearError => {
                    // Resets the state of the error.
                    state.error = None;
                }
                OpCode::JumpIfFalse => {
                    // If the value at the top of the stack is false, jump by the given amount
                    let jump = state.next_primitive()?;
                    if !is_true(state.peek_stack()?) {
                        state.instruction_pointer += jump;
                    }
                }
                OpCode::JumpIfTrue => {
                    // If the value at the top of the stack is true, jump by the given amount
                    let jump = state.next_primitive()?;
                    if is_true(state.peek_stack()?) {
                        state.instruction_pointer += jump;
                    }
                }
                OpCode::JumpIfNotErr => {
                    // If the current state is not in error, jump by the given amount.
                    let jump = state.next_primitive()?;
                    if state.error.is_none() {
                        state.instruction_pointer += jump;
                    }
                }
                OpCode::Jump => {
                    // Moves the instruction pointer by the amount specified
                    let jump = state.next_primitive()?;
                    state.instruction_pointer += jump;
                }
                OpCode::SetPath => {
                    // Sets the path specified by the target to the value at the top of the stack.
                    // The value is then pushed back onto the stack since the assignment expression
                    // also returns this value.
                    // (Allows statements such as `a = b = 32`.
                    let variable = state.next_primitive()?;
                    let variable = &self.targets[variable];
                    let value = state.pop_stack()?;

                    set_variable(ctx, variable, value.clone())?;
                    state.push_stack(value);
                }
                OpCode::SetPathInfallible => {
                    // Sets the path for an infallible assignment statement ie.
                    // thing, err = fallible_call()
                    let variable = state.next_primitive()?;
                    let variable = &self.targets[variable];

                    let error = state.next_primitive()?;
                    let error = &self.targets[error];

                    let default = state.next_primitive()?;
                    let default = &self.values[default];

                    // Note, after assignment the value is pushed back onto the stack since it is possible for
                    // the value to be further used afterwards. This means the value is cloned when the variable is set.
                    // A potential future enhancement would be for the compiler to determine if this value is used and
                    // pass that as a hint to this OpCode so we only clone and fill up the stack when needed.
                    match state.error.take() {
                        Some(err) => {
                            let err = Value::from(err.to_string());
                            set_variable(ctx, variable, default.clone())?;
                            set_variable(ctx, error, err.clone())?;
                            state.push_stack(err);
                        }
                        None => {
                            let value = state.pop_stack()?;
                            set_variable(ctx, variable, value.clone())?;
                            set_variable(ctx, error, Value::Null)?;
                            state.push_stack(value);
                        }
                    }
                }
                OpCode::GetPath => {
                    // Retrieves a value using the given path and pushes this onto the stack.
                    let variable = state.next_primitive()?;
                    let variable = &self.targets[variable];

                    match &variable {
                        Variable::External(path) => {
                            let value = ctx.target().get(path)?.unwrap_or(Value::Null);
                            state.stack.push(value);
                        }
                        Variable::Internal(ident, path) => {
                            let value = match ctx.state().variable(ident) {
                                Some(value) => match path {
                                    Some(path) => {
                                        value.get_by_path(path).cloned().unwrap_or(Value::Null)
                                    }
                                    None => value.clone(),
                                },
                                None => Value::Null,
                            };

                            state.stack.push(value);
                        }
                        Variable::None => state.stack.push(Value::Null),
                        Variable::Stack(path) => {
                            let value = state.pop_stack()?;
                            let value = value.get_by_path(path).cloned().unwrap_or(Value::Null);
                            state.stack.push(value);
                        }
                    }
                }
                OpCode::CreateArray => {
                    // Creates an array from the values on the stack.
                    // The next primitive on the stack is the number of fields in the array
                    // followed by the values to be added to the array.
                    let count = state.next_primitive()?;
                    let mut arr = Vec::new();

                    for _ in 0..count {
                        arr.push(state.pop_stack()?);
                    }
                    arr.reverse();

                    state.stack.push(Value::Array(arr));
                }
                OpCode::CreateObject => {
                    // Creates on object from the values on the stack.
                    // The next primitive on the stack is the number of fields in the object
                    // followed by key, value pairs.
                    let count = state.next_primitive()?;
                    let mut object = BTreeMap::new();

                    for _ in 0..count {
                        let value = state.pop_stack()?;
                        let key = state.pop_stack()?;
                        let key = String::from_utf8_lossy(&key.try_bytes().unwrap()).to_string();

                        object.insert(key, value);
                    }

                    state.stack.push(Value::Object(object));
                }
                OpCode::Call => {
                    // Calls a function in the stdlib.
                    let function_id = state.next_primitive()?;
                    let span_start = state.next_primitive()?;
                    let span_end = state.next_primitive()?;
                    let parameters = &self.fns[function_id].parameters();

                    let len = state.parameter_stack().len();
                    let args = state
                        .parameter_stack_mut()
                        .drain(len - parameters.len()..)
                        .collect();

                    let mut argumentlist = VmArgumentList::new(parameters, args);
                    let function = &self.fns[function_id];

                    let result = argumentlist
                        .check_arguments()
                        .and_then(|_| function.call(ctx, &mut argumentlist));

                    match result {
                        Ok(result) => state.stack.push(result),
                        Err(err) => match err {
                            ExpressionError::Abort { .. } => {
                                panic!("abort errors must only be defined by `abort` statement")
                            }
                            ExpressionError::Error {
                                message,
                                labels,
                                notes,
                            } => {
                                // labels.push(Label::primary(message.clone(), self.span));
                                state.error = Some(ExpressionError::Error {
                                    message: format!(
                                        r#"function call error for "{}" at ({}:{}): {}"#,
                                        function.identifier(),
                                        span_start,
                                        span_end,
                                        message
                                    ),
                                    labels,
                                    notes,
                                });
                            }
                        },
                    }
                }
                OpCode::EmptyParameter => {
                    // Moves an empty, optional parameter onto the parameter stack.
                    state.parameter_stack.push(None)
                }
                OpCode::MoveParameter => {
                    // Moves the top value from the stack onto the parameter stack.
                    state
                        .parameter_stack
                        .push(state.stack.pop().map(VmArgument::Value))
                }
                OpCode::MoveStatic => {
                    // Moves a static parameter onto the parameter stack.
                    // A static parameter will have been created by the functions `compile_argument` method
                    // during compile time.
                    let idx = state.next_primitive()?;
                    state
                        .parameter_stack
                        .push(Some(VmArgument::Any(&self.static_params[idx])));
                }
            }
        }
    }
}

/// Op that applies a function to the top two elements on the stack.
fn binary_op<F, E>(state: &mut VmState, fun: F) -> Result<(), ExpressionError>
where
    E: Into<ExpressionError>,
    F: Fn(Value, Value) -> Result<Value, E>,
{
    // If we are in an error state we don't want to perform the operation
    // so we pass the error along.
    if state.error.is_none() {
        let rhs = state.pop_stack()?;
        let lhs = state.pop_stack()?;
        match fun(lhs, rhs) {
            Ok(value) => state.stack.push(value),
            Err(err) => state.error = Some(err.into()),
        }
    }

    Ok(())
}

/// Sets the value of the given variable to the provided value.
fn set_variable<'a>(
    ctx: &mut Context<'a>,
    variable: &Variable,
    value: Value,
) -> Result<(), ExpressionError> {
    match variable {
        Variable::Internal(ident, path) => {
            let path = match path {
                Some(path) => path,
                None => {
                    ctx.state_mut().insert_variable(ident.clone(), value);
                    return Ok(());
                }
            };

            // Update existing variable using the provided path, or create a
            // new value in the store.
            match ctx.state_mut().variable_mut(ident) {
                Some(stored) => stored.insert_by_path(path, value),
                None => ctx
                    .state_mut()
                    .insert_variable(ident.clone(), value.at_path(path)),
            }
        }
        Variable::External(path) => ctx.target_mut().insert(path, value)?,

        // Setting these cases should not be allowed by the compiler
        Variable::None | Variable::Stack(_) => (),
    }

    Ok(())
}

fn is_true(object: &Value) -> bool {
    matches!(object, Value::Boolean(true))
}