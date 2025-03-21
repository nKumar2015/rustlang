use std::collections::HashMap;
use std::env::{ args, current_dir, var};
use std::path::Path;

use crate::ast::{Expression, IfBranch, ListItem, Operator, Program, Statement};
use crate::parser::ProgramParser;
use crate::read_file;
use crate::value::Value;

pub fn eval_program(enviornment: &mut HashMap<String, Value>, 
                    Program::Body{statements}: &Program, importing: bool) 
                    -> Result<(), String> {
        
        eval_statements(enviornment, statements, importing)
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
        },
        Expression::Index { name, idx_exp} => {
            let Some(var) = enviornment.get(&name) 
                else { return Err(format!("'{}' is not defined", name)) };
            

            let exp_res = 
                match eval_expression(&mut enviornment.clone(), 
                          &idx_exp, false){
                    Ok(v) => v,
                    Err(e) => return Err(e),
            };

            let mut list = match var {
                Value::List { e } => e.clone(),

                Value::Str { .. } 
                    => return Err("Cannot assign to String Index".to_string()),
                Value::Null 
                    => return Err("Cannot index Null".to_string()),
                Value::Int { .. } 
                    => return Err("Cannot index Int".to_string()),
                Value::Bool { .. } 
                    => return Err("Cannot index Boolean".to_string()),
                Value::Char { .. } 
                    => return Err("Cannot index Char".to_string()),
                Value::Function { .. } 
                    => return Err("Cannot index Function".to_string()),
                Value::UserDefFunction { .. } 
                    => return Err("Cannot index Function".to_string()),
                Value::Float { .. } 
                    => return Err("Cannot index Float".to_string()),
            };

            let Value::Int { v: idx } = exp_res 
                else { return Err("Index must be of type int".to_string()) };

            let usize_idx = idx.unsigned_abs() as usize;
            let length = list.len();
            if usize_idx > length {
                return Err(format!("Index {} is out of bounds", idx));
            }

            if idx < 0 {
                list[length - usize_idx] = rhs;
            }else{
                list[usize_idx] = rhs;
            }
            

            enviornment.insert(name, Value::List { e: list });
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
        Expression::Comprehension { .. } 
            => return Err("Cannot assign to a Comprehension".to_string()),
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
    statement: &Statement, importing: bool) -> Result<(), String> {
    match statement {
        Statement::Expression{expression} => {
            eval_expression(enviornment, expression, importing)?;
        },
        Statement::Assignment{lhs, rhs} => {
            let v = 
                match eval_expression(enviornment, rhs, importing) {
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

            let rhs = match eval_expression(enviornment, rhs, importing) {
                    Ok(v) => v,
                    Err(e) => return Err(e)
                };

            let v = 
                match operate(operator, &lhs, &rhs) {
                    Ok(Value::Null) 
                        => return Err(format!("Cannot operate on {}", name)),
                    Ok(v) => v,
                    Err(e) => return Err(e)
                };

            enviornment.insert(name.clone(), v);
        },
        Statement::If{params} => {
            match eval_expression(enviornment, &params.condition, importing) {
                Ok(Value::Bool{b: true}) 
                    => eval_statements(enviornment, &params.statements, 
                                       importing)?,
                Ok(Value::Bool{b: false}) => {
                    let (elif_conditions, elif_statements ) = &params.elif_data;
                    if !elif_conditions.is_empty() {
                        let condition = elif_conditions[0].clone();
                        let statement = elif_statements[0].clone();

                        let next_iter = IfBranch{
                            condition,
                            statements: statement,
                            else_statements: params.else_statements.clone(),
                            elif_data: (elif_conditions[1..].to_vec(), 
                                        elif_statements[1..].to_vec())
                        };

                        eval_statement(enviornment, 
                            &Statement::If{params: next_iter}, importing)?;
                    }else if let Some(else_statements) = 
                        &params.else_statements { 
                            eval_statements(enviornment, else_statements, 
                                            importing)?;
                    }
                },
                _ => return Err("Condition must be of type 'bool'".to_string()),
            }
        },
        Statement::While{condition, statements} => {            
            loop{
                let b = 
                    match eval_expression(enviornment, condition, importing) {
                        Ok(Value::Bool{b}) => b ,
                        Err(e) => return Err(e),
                        _ => return Err(
                            "Condition must be of type 'bool'".to_string()),
                    };
                            
                if !b { break; }
                
                if let Err(e) 
                    = eval_statements(enviornment, statements, importing) {
                    return Err(e);
                }
            }
        },
        Statement::For{params} => {
            let v = 
            match &params.iterate_expression {
                Expression::List { .. } 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression, importing)?,
                Expression::Identifier { .. } 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression, importing)?,
                Expression::Call { .. } 
                    => eval_expression(enviornment, 
                                      &params.iterate_expression, importing)?,
                Expression::Int { .. } 
                    => return Err(
                        "Integer literals are not iterable".to_string()),
                Expression::String { .. } 
                    => return Err(
                        "String literals are not iterable".to_string()),
                Expression::Boolean { .. } 
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
                Expression::Index { .. } 
                    => return Err(
                        "Indexes are not iterable".to_string()),
                Expression::Comprehension { .. } 
                    => return Err(
                        "Comprehensions are not iterable".to_string())
            };

            let Value::List{e: iterator_list} = v 
                else { return Err("Invalid Type".to_string())};

            for list_item in iterator_list {
                enviornment.insert(params.loop_var.clone(), list_item);

                eval_statements(enviornment, &params.statements, importing)?;
            }
        },
        Statement::FunctionDefinition { name, arguments, 
                                        statements, return_expression } => {
            if enviornment.get(name).is_some() {
                return Err("Function '{}' is already defined!".to_string());
            }

            enviornment.insert(name.to_string(), 
                               Value::UserDefFunction { 
                                    name: name.to_string(),
                                    statements: statements.clone(),
                                    arguments: arguments.clone(),
                                    return_expression: return_expression.clone()
                                });
        },
        Statement::Import{path} => {    
            // Get the provided path to file 
            // and the directory the executable was called from

            let args: Vec<String> = args().collect();
            let cwd = current_dir().unwrap();
            
            // The provided path
            let origin_file: &String = &args[1];

            // replace "." with the current working directory
            let mut full_path = origin_file.clone();
            if full_path.starts_with('.') {
                full_path = origin_file.replacen('.', 
                                    cwd.to_str().unwrap(),
                                    1);
            }

            let external_code = 
                if path.starts_with('.') {                    
                    // Move one level up
                    let parent_dir 
                        = Path::new(&full_path).parent().unwrap();

                    // replace the "." from the provided import path with the
                    // parent directory we found earlier
                    let full_import_path = 
                        path.replacen('.', 
                                    parent_dir.to_str().unwrap(), 
                                    1);
                    
                    // attempt to read that file
                    match read_file(&full_import_path) {
                        Ok(f) => f,
                        Err(_) => 
                            return Err(format!("Error opening file at {}", 
                                            full_import_path))
                    } 
                } else if path.contains('/'){
                    match read_file(path) {
                        Ok(f) => f,
                        Err(_) => return 
                            Err(format!("Error opening file at {}", path))
                    } 
                } else {
                    // Move one level up
                    let parent_dir 
                        = Path::new(&full_path).parent().unwrap();
                    let final_dir 
                        = format!("{}/{}", parent_dir.to_str().unwrap(), path); 
                    let result = read_file(&final_dir);

                    // If the file is present in the same directory, use that
                    #[allow(clippy::unnecessary_unwrap)]
                    if result.is_ok() {
                        result.unwrap()
                    }else {
                        // If the file is not present, check if the file exists 
                        // in the paths listedn inthe RUSTL_LIB env var 
                        let var = var("RUSTL_LIB");
                        let mut out = String::new();
                        if var.is_ok(){
                            let res_val = var.unwrap();
                            let paths = res_val.split(':');
                            for dir in paths {
                                let lib_path = format!("{}/{}", dir, path);
                                let res = read_file(&lib_path);

                                if res.is_ok() {
                                    out = res.unwrap();
                                    break;
                                }
                            }
                        }
                        if out.is_empty() {
                            return Err(format!("Error opening file at {}", 
                                       path));
                        }
                        out.to_string()
                    }
                };
            let ast = ProgramParser::new().parse(&external_code).unwrap();

            eval_program(enviornment, &ast, true)?;
        },
        //_ => return Err(format!("unhandled statement: {:?}", statement)),
    }

    Ok(())
}

fn eval_statements(enviornment: &mut HashMap<String, Value>, 
                   statements: &Vec<Statement>, 
                   importing: bool) -> Result<(), String> {
    
    for statement in statements {
        eval_statement(enviornment, statement, importing)?;
    }

    Ok(())
}

fn eval_expression(enviornment: &mut HashMap<String, Value>, 
    expression: &Expression, importing: bool) -> Result<Value, String>{
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
            let vals = eval_expressions(enviornment, arguments, importing)?;

            let Some(v) = enviornment.get(function) 
                else { return Err(format!("'{}' is not defined", &function)) };
            
            let mut local_env = enviornment.clone();

            match v {
                Value::Function{f, ..} => {
                    if importing && (function == "print" || 
                                     function == "println" ) {

                            return Ok(Value::Null);     
                    }
                    f(vals)
                },
                Value::UserDefFunction {statements, 
                                        arguments , return_expression, ..} => {
                    if vals.len() != arguments.len() {
                        return Err(format!("Expected {} arguments, got {}", 
                                            arguments.len(), 
                                            vals.len()))
                    }
                    for (value, name) in vals.iter().zip(arguments.iter()) {
                        local_env.insert(name.to_string(), value.clone());
                    }
                    eval_statements(&mut local_env, statements, importing)?;
                    
                    match return_expression {
                        Some(return_exp) => {
                            match eval_expression(&mut enviornment.clone(),
                                      return_exp, importing) {
                                Ok(v) => Ok(v.clone()),
                                Err(e) 
                                    => Err(e)
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
                match eval_expression(enviornment, expression, importing) {
                    Ok(v) => vals.push(v),
                    Err(e) => return Err(e),
                }
            }

            if let [lhs, rhs] = vals.as_slice() {
                let new_val = operate(operator, lhs, rhs)?;
                if new_val == Value::Null {
                    return Err("Invalid Operation".to_string())
                }
                Ok(new_val)
            }else{
                Err("dev error: ".to_string())
            }
        },
        Expression::List { items} => {
            let mut vals: Vec<Value> = vec![];
            
            for item in items {
                let v = 
                    match eval_expression(enviornment, 
                                          &item.expression, 
                                          importing) {
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

            let v = match eval_expression(enviornment, rhs, importing) {
                Ok(v) => v,
                Err(e) => return Err(e)
            };

            let new_val = operate(operator, &lhs, &v)?;
            if new_val == Value::Null {
                return Err(format!("Cannot operate on {}", name))
            }
            enviornment.insert(name.clone(), new_val.clone());

            Ok(new_val)
        },
        Expression::Index { name, idx_exp } => {
            let Some(var) = enviornment.get(name) 
                else { return Err(format!("'{}' is not defined", name)) };

            let exp_res = eval_expression(&mut enviornment.clone(), idx_exp, 
                                                 importing)?;

            let Value::Int { v: idx } = exp_res 
                else { return Err("Index must be of type int".to_string()) };

            let mut iterator = var.clone().into_iter();
            let length = iterator.clone().count();

            if iterator.value == Value::Null {
                return Err(format!("Cannot iterate over variable {}", name))
            }

            let usize_idx = idx.unsigned_abs() as usize;

            if usize_idx > length {
                return Err(format!("Index {} is out of bounds", idx))
            }

            if idx < 0 {
                return Ok(iterator.nth(length - usize_idx)
                    .unwrap_or_else(|| panic!("Err retreiving value at {}", 
                                               idx)))
            }

            Ok(iterator.nth(usize_idx)
                .unwrap_or_else(|| panic!("Err retreiving value at {}", idx)))
        },
        Expression::Comprehension { iterate_exp, var, control_exp } => {
            let mut local_env = enviornment.clone();
            let control_val = eval_expression(&mut local_env, 
                                                      control_exp, importing)?;

            match control_val {
                Value::List { e } => {
                    let mut output = vec![];
                    for item in e {
                        local_env.insert(var.to_string(), item);
                        let iterate_exp_val = 
                            eval_expression(&mut local_env, 
                                             iterate_exp, importing)?;
                        output.push(iterate_exp_val);
                    }
                    Ok(Value::List{e: output})
                },
                Value::Str { s } => {
                    let mut output = vec![];
                    for c in s.chars() {
                        local_env.insert(var.to_string(), Value::Char {c});
                        let iterate_exp_val = 
                            eval_expression(&mut local_env, 
                                             iterate_exp, importing)?;

                        output.push(iterate_exp_val);
                    }
                    Ok(Value::List{e: output})
                },
                Value::Null 
                    => Err("Null is not iterable".to_string()),
                Value::Int { .. } 
                    => Err("Int is not iterable".to_string()),
                Value::Bool { .. } 
                    => Err("Bool is not iterable".to_string()),
                Value::Float { .. } 
                    => Err("Float is not iterable".to_string()),
                Value::Char { .. } 
                    => Err("Char is not iterable".to_string()),
                Value::Function { .. } 
                    => Err("Function is not iterable".to_string()),
                Value::UserDefFunction { .. } 
                    => Err("Function is not iterable".to_string()),
            }
        }
        //_=> Err(format!("unhandled expression: {:?}", expression)),
    }
}

fn eval_expressions(enviornment: &mut HashMap<String, Value>, 
                    expressions: &Vec<Expression>, 
                    importing: bool) -> Result<Vec<Value>, String> {
        let mut vals = vec![];

        for expression in expressions {
            match eval_expression(enviornment, expression, importing) {
                Ok(v) => vals.push(v),
                Err(e) => return Err(e),
            }
        }

        Ok(vals)
}

fn operate(operator: &Operator, lhs: &Value, rhs: &Value) 
    -> Result<Value, String>{
        match operator {
            Operator::Plus => Ok(lhs + rhs),
            Operator::Minus => Ok(lhs + rhs),
            Operator::Times => Ok(lhs + rhs),
            Operator::Divide => Ok(lhs + rhs),
            Operator::LessThan => Ok(lhs + rhs),
            Operator::GreaterThan => Ok(lhs + rhs),
            Operator::Equal => Ok(Value::Bool{b: lhs == rhs}),
            Operator::NotEqual => Ok(Value::Bool{b: lhs != rhs}),
        }
}
              
