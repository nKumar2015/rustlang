use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};

use crate::ast::{Expression, ListItem, Operator, Program, Statement};
use crate::constants::FP_ERROR_MARGIN;
use crate::parser::ProgramParser;

pub fn eval_program(enviornment: &mut HashMap<String, Value>, 
    Program::Body{statements}: &Program) -> Result<(), String> {
        
        eval_statements(enviornment, statements)
}

fn assign(enviornment: &mut HashMap<String, Value>, lhs: Expression, rhs: Value)
    -> Result<(), String> {

    match lhs {
        Expression::Identifier { name } => {
            if name == "_" {
                return Ok(());
            }
            enviornment.insert(name.clone(), rhs);
        },
        
        Expression::List { items } => {
            let Value::List{e: new_items} = rhs 
            else { 
                return Err("cannot destructure non-list into list".to_string()) 
            };

            assign_list(enviornment, items, new_items)?;
        }

        Expression::Int { .. } 
            => return Err("Cannot assign to a Integer literal".to_string()),
        Expression::String { .. } 
            => return Err("Cannot assign to a String literal".to_string()),
        Expression::Boolean { ..} 
            => return Err("Cannot assign to a Boolean literal".to_string()),
        Expression::Float { .. } 
            => return Err("Cannot assign to a Float literal".to_string()),
        Expression::Character { .. } 
            => return Err("Cannot assign to a Character literal".to_string()),
        Expression::Call { ..} 
            => return Err("Cannot assign to a Function call".to_string()),
        Expression::Operation { .. } 
            => return Err("Cannot assign to a Operation".to_string()),
        Expression::Prefix { .. } 
            => return Err("Cannot assign to a Prefix".to_string()),
    }



    Ok(())
}

fn assign_list(enviornment: &mut HashMap<String, Value>, lhs: Vec<ListItem>, 
    rhs: Vec<Value>) -> Result<(), String> {

    if lhs.len() > rhs.len() {
        return Err(format!("Cannot assign {} values to {} items", 
                    rhs.len(), 
                    lhs.len()))
    }

    let mut assign_name_queue: Vec<ListItem> = vec![];
    let mut assign_value_queue: Vec<Value> = vec![];

    for x in 0..rhs.len(){
        if x == lhs.len() - 1 && lhs.len() != rhs.len(){
            if !lhs[x].is_pack {
                return Err(format!("Cannot assign {} values to {} items", 
                    rhs.len(), 
                    lhs.len()))
            }

            assign_name_queue.push(lhs[x].clone());
            assign_value_queue.push(Value::List{e: rhs[x..].to_vec()});
            break;
        }

        if lhs[x].is_spread {
            return Err("Cannot use spread in list assignment".to_string())
        }

        assign_name_queue.push(lhs[x].clone());
        assign_value_queue.push(rhs[x].clone());
    }

    for (ListItem{expression, .. }, value) in
        assign_name_queue.into_iter().zip(assign_value_queue.into_iter()) {
        
        assign(enviornment, expression, value)?;
    }

    Ok(())

}

fn eval_statement(enviornment: &mut HashMap<String, Value>, 
    statement: &Statement) -> Result<(), String> {
    match statement {
        Statement::Expression{expression} => {
            eval_expression(enviornment, expression)?;
        },
        Statement::Assignment{lhs, rhs} => {
            let v = 
                match eval_expression(enviornment, rhs) {
                    Ok(v) => v,
                    Err(e) => return Err(e),
                };
            
            assign(enviornment, lhs.clone(), v)?;
        },
        Statement::OperatorAssignment{name, operator, rhs} => {
            let lhs = 
                match enviornment.get(name) {
                    Some(v) => v.clone(),
                    None => return Err(format!("'{}' is not defined", &name))
                };

            let rhs = match eval_expression(enviornment, rhs) {
                    Ok(v) => v,
                    Err(e) => return Err(e)
                };

            let v = 
                match operate(operator, &lhs, &rhs) {
                    Ok(v) => v,
                    Err(e) => return Err(e)
                };

            enviornment.insert(name.clone(), v);
        },
        Statement::If{params} => {
            match eval_expression(enviornment, &params.condition) {
                Ok(Value::Bool{b: true}) 
                    => eval_statements(enviornment, &params.statements)?,
                Ok(Value::Bool{b: false}) 
                    => {
                        if let Some(else_statements) 
                            = &params.else_statements { 
                            eval_statements(enviornment, else_statements)?;
                        }
                    },
                _ => return Err("Condition must be of type 'bool'".to_string()),
            }
        },
        Statement::While{condition, statements} => {            
            loop{
                let b = 
                    match eval_expression(enviornment, condition) {
                        Ok(Value::Bool{b}) => b ,
                        Err(e) => return Err(e),
                        _ => return Err(
                            "Condition must be of type 'bool'".to_string()),
                    };
                            
                if !b { break; }
                
                #[allow(clippy::question_mark)]
                if let Err(e) = eval_statements(enviornment, statements) {
                    return Err(e);
                }
            }
        },
        Statement::For{params} => {
            let v = 
            match &params.iterate_expression {
                Expression::List { .. } 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression)?,
                Expression::Identifier { .. } 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression)?,
                Expression::Call { ..} 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression)?,
                Expression::Int { .. } 
                    => return Err(
                        "Integer literals are not iterable".to_string()),
                Expression::String { .. } 
                    => return Err(
                        "String literals are not iterable".to_string()),
                Expression::Boolean { ..} 
                    => return Err(
                        "Boolean literals are not iterable".to_string()),
                Expression::Float { .. } 
                    => return Err(
                        "Float literals are not iterable".to_string()),
                Expression::Character { .. } 
                    => return Err(
                        "Character literals are not iterable".to_string()),
                Expression::Operation { .. } 
                    => return Err(
                        "Operations are not iterable".to_string()),
                Expression::Prefix { .. } 
                        => return Err(
                            "Prefix's are not iterable".to_string()),
            };

            let Value::List{e: iterator_list} = v 
                else { return Err("Invalid Type".to_string())};

            for list_item in iterator_list {
                enviornment.insert(params.loop_var.clone(), list_item);

                eval_statements(enviornment, &params.statements)?;
            }
        },
        Statement::FunctionDefinition { name, arguments, 
                                        statements, return_val } => {
            if enviornment.get(name).is_some() {
                return Err("Function '{}' is already defined!".to_string());
            }

            enviornment.insert(name.to_string(), 
                               Value::UserDefFunction { 
                                    statements: statements.clone(),
                                    arguments: arguments.clone(),
                                    return_val: return_val.clone()
                                });
        },
        Statement::Import{path} => {
            let file = File::open(path).unwrap();
            let lines = BufReader::new(file).lines();
            let mut external_code = String::new();

            for s in lines {
                external_code.push_str(&s.unwrap());
            }

            let ast = ProgramParser::new().parse(&external_code).unwrap();

            eval_program(enviornment, &ast)?;
        }
        //_ => return Err(format!("unhandled statement: {:?}", statement)),
    }

    Ok(())
}

fn eval_statements(enviornment: &mut HashMap<String, Value>, 
              statements: &Vec<Statement>) -> Result<(), String> {
    
    for statement in statements {
        eval_statement(enviornment, statement)?;
    }

    Ok(())
}

fn eval_expression(enviornment: &mut HashMap<String, Value>, 
    expression: &Expression) -> Result<Value, String>{
    match expression {
        Expression::Int{v} => Ok(Value::Int{v: *v}),
        Expression::String{ s } => Ok(Value::Str{s: s.clone()}),
        Expression::Boolean{ b } => Ok(Value::Bool{b: *b}),
        Expression::Float{ f} => Ok(Value::Float{f: *f}),
        Expression::Character{ c } => Ok(Value::Char{c: *c}),
        Expression::Identifier{name} => {
            match enviornment.get(name) {
                Some(v) => Ok(v.clone()),
                None => Err(format!("'{}' is not defined", &name))
            }
        },
        Expression::Call{function, arguments} =>  {
            let vals = eval_expressions(enviornment, arguments)?;

            let Some(v) = enviornment.get(function) 
                else { return Err(format!("'{}' is not defined", &function)) };
            
            let mut local_env = enviornment.clone();

            match v {
                Value::Function{f} => {
                    f(vals)
                },
                Value::UserDefFunction {statements, 
                                        arguments , return_val} => {
                    if vals.len() != arguments.len() {
                        return Err(format!("Expected {} arguments, got {}", 
                                            arguments.len(), 
                                            vals.len()))
                    }
                    for (value, name) in vals.iter().zip(arguments.iter()) {
                        local_env.insert(name.to_string(), value.clone());
                    }
                    eval_statements(&mut local_env, statements)?;
                    
                    match return_val {
                        Some(name) => {
                            match local_env.get(name) {
                                Some(v) => Ok(v.clone()),
                                None 
                                    => Err(format!("'{}' is not defined", name))
                            }
                        },
                        None => Ok(Value::Null)
                    }

                },
                _ => Err(format!("'{function}' is not a function"))
            }
        },
        Expression::Operation { lhs, rhs, operator } => {
            let expressions = vec![lhs, rhs];
            let mut vals = vec![];

            for expression in expressions {
                match eval_expression(enviornment, expression) {
                    Ok(v) => vals.push(v),
                    Err(e) => return Err(e),
                }
            }

            if let [lhs, rhs] = vals.as_slice() {
                operate(operator, lhs, rhs)
            }else{
                Err("dev error: ".to_string())
            }
        },
        Expression::List { items} => {
            let mut vals: Vec<Value> = vec![];
            
            for item in items {
                let v = 
                    match eval_expression(enviornment, &item.expression) {
                        Ok(v) => v,
                        Err(e) => return Err(e)
                    };

                if !item.is_spread {
                    vals.push(v);
                    continue;
                }

                match v {
                    Value::List{mut e} => vals.append(&mut e),
                    _ => return Err("only lists can be spread!".to_string())
                }
            }

            Ok(Value::List{e: vals})
        },
        Expression::Prefix { name, operator, rhs } => {
            let lhs = match enviornment.get(name) {
                Some(v) => v.clone(),
                None => return Err(format!("'{}' is not defined", name))
            };

            let v = match eval_expression(enviornment, rhs) {
                Ok(v) => v,
                Err(e) => return Err(e)
            };

            let new_val = operate(operator, &lhs, &v)?;

            enviornment.insert(name.clone(), new_val.clone());

            Ok(new_val)
        },
        //_=> Err(format!("unhandled expression: {:?}", expression)),
    }
}

fn eval_expressions(enviornment: &mut HashMap<String, Value>, 
    expressions: &Vec<Expression>) -> Result<Vec<Value>, String> {
        let mut vals = vec![];

        for expression in expressions {
            match eval_expression(enviornment, expression) {
                Ok(v) => vals.push(v),
                Err(e) => return Err(e),
            }
        }

        Ok(vals)
}

fn operate(operator: &Operator, lhs: &Value, rhs: &Value) 
    -> Result<Value, String>{
    match (lhs, rhs){
        (Value::Int{v: lhs}, Value::Int{v: rhs}) => {
            match operator {
                Operator::Plus => {Ok(Value::Int { v: lhs + rhs })},
                Operator::Minus => {Ok(Value::Int { v: lhs - rhs })},
                Operator::Times => {Ok(Value::Int { v: lhs * rhs })},
                Operator::Divide => {Ok(Value::Int { v: lhs / rhs })},
                Operator::LessThan => {Ok(Value::Bool { b: lhs < rhs })},
                Operator::GreaterThan => {Ok(Value::Bool { b: lhs > rhs })},
                Operator::Equal => {Ok(Value::Bool { b: lhs == rhs })},
                Operator::NotEqual => {Ok(Value::Bool { b: lhs != rhs })}
            } 
        },
        (Value::Float{f: lhs}, Value::Float{f: rhs}) => {
            match operator{
                Operator::Plus => {Ok(Value::Float { f: lhs + rhs })},
                Operator::Minus => {Ok(Value::Float { f: lhs - rhs })},
                Operator::Times => {Ok(Value::Float { f: lhs * rhs })},
                Operator::Divide => {Ok(Value::Float { f: lhs / rhs })},
                Operator::LessThan => {Ok(Value::Bool { b: lhs < rhs })},
                Operator::GreaterThan => {Ok(Value::Bool { b: lhs > rhs })},
                Operator::Equal => {Ok(Value::Bool { 
                    b: (lhs - rhs).abs() < FP_ERROR_MARGIN 
                })},
                Operator::NotEqual => {Ok(Value::Bool { 
                    b: (lhs - rhs).abs() > FP_ERROR_MARGIN 
                })},
            }
        },
        (Value::Float{f: lhs}, Value::Int{v: rhs}) => {
            let rhsf = f64::from(*rhs);
            match operator{
                Operator::Plus => {Ok(Value::Float { f: lhs + rhsf })},
                Operator::Minus => {Ok(Value::Float { f: lhs - rhsf })},
                Operator::Times => {Ok(Value::Float { f: lhs * rhsf })},
                Operator::Divide => {Ok(Value::Float { f: lhs / rhsf })},
                Operator::LessThan => {Ok(Value::Bool { b: *lhs < rhsf })},
                Operator::GreaterThan => {Ok(Value::Bool { b: *lhs > rhsf })},
                Operator::Equal => {Ok(Value::Bool { 
                    b: (*lhs - rhsf).abs() < FP_ERROR_MARGIN 
                })},
                Operator::NotEqual => {Ok(Value::Bool { 
                    b: (*lhs - rhsf).abs() > FP_ERROR_MARGIN 
                })},

            }
        },
        (Value::Int{v: lhs}, Value::Float{f: rhs}) => {
            let lhsf = f64::from(*lhs);
            match operator{
                Operator::Plus => {Ok(Value::Float { f: lhsf + rhs })},
                Operator::Minus => {Ok(Value::Float { f: lhsf - rhs })},
                Operator::Times => {Ok(Value::Float { f: lhsf * rhs })},
                Operator::Divide => {Ok(Value::Float { f: lhsf / rhs })},
                Operator::LessThan => {Ok(Value::Bool { b: lhsf < *rhs })},
                Operator::GreaterThan => {Ok(Value::Bool { b: lhsf > *rhs })},
                Operator::Equal => {Ok(Value::Bool { 
                    b: (lhsf - *rhs).abs() < FP_ERROR_MARGIN 
                })},
                Operator::NotEqual => {Ok(Value::Bool { 
                    b: (lhsf - *rhs).abs() > FP_ERROR_MARGIN 
                })},
            }
        }
        _ => Err(format!("unhandled types: {:?}", (lhs, rhs))),
    }
}               

#[derive(Clone,Debug)]
pub enum Value {
    Null,
    Int{v: i32},
    #[allow(dead_code)]
    Str{s: String},
    Bool{b: bool},
    #[allow(dead_code)]
    Float{f: f64},
    #[allow(dead_code)]
    Char{c: char},
    #[allow(dead_code)]
    List{e: Vec<Value>},
    Function{f: fn(Vec<Value>) -> Result<Value, String>},
    #[allow(dead_code)]
    UserDefFunction{statements: Vec<Statement>, arguments: Vec<String>, 
                    return_val: Option<String> }
}