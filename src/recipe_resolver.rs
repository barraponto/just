use crate::common::*;

use CompilationErrorKind::*;

pub(crate) struct RecipeResolver<'a: 'b, 'b> {
  unresolved_recipes: Table<'a, Recipe<'a, Name<'a>>>,
  resolved_recipes: Table<'a, Rc<Recipe<'a>>>,
  assignments: &'b Table<'a, Assignment<'a>>,
}

impl<'a, 'b> RecipeResolver<'a, 'b> {
  pub(crate) fn resolve_recipes(
    unresolved_recipes: Table<'a, Recipe<'a, Name<'a>>>,
    assignments: &Table<'a, Assignment<'a>>,
  ) -> CompilationResult<'a, Table<'a, Rc<Recipe<'a>>>> {
    let mut resolver = RecipeResolver {
      resolved_recipes: empty(),
      unresolved_recipes,
      assignments,
    };

    while let Some(unresolved) = resolver.unresolved_recipes.pop() {
      resolver.resolve_recipe(&mut Vec::new(), unresolved)?;
    }

    for recipe in resolver.resolved_recipes.values() {
      for parameter in &recipe.parameters {
        if let Some(expression) = &parameter.default {
          for (function, argc) in expression.functions() {
            Function::resolve(&function, argc)?;
          }
          for variable in expression.variables() {
            resolver.resolve_variable(&variable, &[])?;
          }
        }
      }

      for line in &recipe.body {
        for fragment in &line.fragments {
          if let Fragment::Interpolation { expression, .. } = fragment {
            for (function, argc) in expression.functions() {
              Function::resolve(&function, argc)?;
            }
            for variable in expression.variables() {
              resolver.resolve_variable(&variable, &recipe.parameters)?;
            }
          }
        }
      }
    }

    Ok(resolver.resolved_recipes)
  }

  fn resolve_variable(
    &self,
    variable: &Token<'a>,
    parameters: &[Parameter],
  ) -> CompilationResult<'a, ()> {
    let name = variable.lexeme();
    let undefined =
      !self.assignments.contains_key(name) && !parameters.iter().any(|p| p.name.lexeme() == name);

    if undefined {
      return Err(variable.error(UndefinedVariable { variable: name }));
    }

    Ok(())
  }

  fn resolve_recipe(
    &mut self,
    stack: &mut Vec<&'a str>,
    recipe: Recipe<'a, Name<'a>>,
  ) -> CompilationResult<'a, Rc<Recipe<'a>>> {
    if let Some(resolved) = self.resolved_recipes.get(recipe.name()) {
      return Ok(resolved.clone());
    }

    stack.push(recipe.name());

    let mut dependencies: Vec<Dependency> = Vec::new();
    for dependency in &recipe.dependencies {
      let name = dependency.lexeme();

      if let Some(resolved) = self.resolved_recipes.get(name) {
        // dependency already resolved
        if !resolved.parameters.is_empty() {
          return Err(dependency.error(DependencyHasParameters {
            recipe: recipe.name(),
            dependency: name,
          }));
        }

        dependencies.push(Dependency(resolved.clone()));
      } else if stack.contains(&name) {
        let first = stack[0];
        stack.push(first);
        return Err(
          dependency.error(CircularRecipeDependency {
            recipe: recipe.name(),
            circle: stack
              .iter()
              .skip_while(|name| **name != dependency.lexeme())
              .cloned()
              .collect(),
          }),
        );
      } else if let Some(unresolved) = self.unresolved_recipes.remove(name) {
        // resolve unresolved dependency
        if !unresolved.parameters.is_empty() {
          return Err(dependency.error(DependencyHasParameters {
            recipe: recipe.name(),
            dependency: name,
          }));
        }

        dependencies.push(Dependency(self.resolve_recipe(stack, unresolved)?));
      } else {
        // dependency is unknown
        return Err(dependency.error(UnknownDependency {
          recipe: recipe.name(),
          unknown: name,
        }));
      }
    }

    let resolved = Rc::new(recipe.resolve(dependencies));
    self.resolved_recipes.insert(resolved.clone());
    stack.pop();
    Ok(resolved)
  }
}

#[cfg(test)]
mod tests {
  use super::*;

  analysis_error! {
    name:   circular_recipe_dependency,
    input:  "a: b\nb: a",
    offset: 8,
    line:   1,
    column: 3,
    width:  1,
    kind:   CircularRecipeDependency{recipe: "b", circle: vec!["a", "b", "a"]},
  }

  analysis_error! {
    name:   self_recipe_dependency,
    input:  "a: a",
    offset: 3,
    line:   0,
    column: 3,
    width:  1,
    kind:   CircularRecipeDependency{recipe: "a", circle: vec!["a", "a"]},
  }

  analysis_error! {
    name:   unknown_dependency,
    input:  "a: b",
    offset: 3,
    line:   0,
    column: 3,
    width:  1,
    kind:   UnknownDependency{recipe: "a", unknown: "b"},
  }

  analysis_error! {
    name:   unknown_interpolation_variable,
    input:  "x:\n {{   hello}}",
    offset: 9,
    line:   1,
    column: 6,
    width:  5,
    kind:   UndefinedVariable{variable: "hello"},
  }

  analysis_error! {
    name:   unknown_second_interpolation_variable,
    input:  "wtf=\"x\"\nx:\n echo\n foo {{wtf}} {{ lol }}",
    offset: 33,
    line:   3,
    column: 16,
    width:  3,
    kind:   UndefinedVariable{variable: "lol"},
  }

  analysis_error! {
    name:   unknown_function_in_interpolation,
    input:  "a:\n echo {{bar()}}",
    offset: 11,
    line:   1,
    column: 8,
    width:  3,
    kind:   UnknownFunction{function: "bar"},
  }

  analysis_error! {
    name:   unknown_function_in_default,
    input:  "a f=baz():",
    offset: 4,
    line:   0,
    column: 4,
    width:  3,
    kind:   UnknownFunction{function: "baz"},
  }

  analysis_error! {
    name:   unknown_variable_in_default,
    input:  "a f=foo:",
    offset: 4,
    line:   0,
    column: 4,
    width:  3,
    kind:   UndefinedVariable{variable: "foo"},
  }
}
