use value::Value;
use std::borrow::Borrow;

pub struct Arena {
  values: Vec<ArenaValue>,
}

#[derive(Debug, PartialEq)]
enum ArenaValue {
  Absent,
  Present(Box<Value>),
}

impl Arena {
  pub fn intern(&mut self, v: Value) -> usize {
    let space = self.find_space();
    match space {
      Some(n) => {
        self.values[n] = ArenaValue::Present(Box::new(v));
        n
      }
      None => {
        self.values.push(ArenaValue::Present(Box::new(v)));
        self.values.len() - 1
      }
    }
  }

  pub fn value_ref(&mut self, at: usize) -> &Value {
    match self.values[at] {
      ArenaValue::Absent => panic!("mutable_box on absent value."),
      ArenaValue::Present(ref b) => b.borrow()
    }
  }

  pub fn new() -> Arena {
    Arena { values: Vec::new() }
  }

  /// Returns the address of the first `Absent` value in the arena, or an empty optional if there
  /// is none.
  fn find_space(&self) -> Option<usize> {
    self.values.iter()
        .enumerate()
        .filter(|(_index, value)| **value == ArenaValue::Absent)
        .nth(0)
        .map(|(index, _value)| index)
  }
}

#[cfg(test)]
mod tests {
  use super::*;
  use value::Value;

  #[test]
  fn add_empty() {
    let mut arena = Arena::new();
    assert_eq!(0, arena.intern(Value::EmptyList));
  }

  #[test]
  fn add_remove() {
    let mut arena = Arena::new();
    assert_eq!(0, arena.intern(Value::EmptyList));
    assert_eq!(1, arena.intern(Value::EmptyList));
    assert_eq!(2, arena.intern(Value::EmptyList));
    arena.values[1] = ArenaValue::Absent;
    assert_eq!(1, arena.intern(Value::EmptyList));
  }

  #[test]
  fn get() {
    let mut arena = Arena::new();
    assert_eq!(0, arena.intern(Value::Real(0.1)));
    assert_eq!(Value::Real(0.1), *arena.value_ref(0));
  }
}
