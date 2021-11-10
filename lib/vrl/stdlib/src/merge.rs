use std::collections::BTreeMap;

use vrl::prelude::*;

#[derive(Clone, Copy, Debug)]
pub struct Merge;

impl Function for Merge {
    fn identifier(&self) -> &'static str {
        "merge"
    }

    fn parameters(&self) -> &'static [Parameter] {
        &[
            Parameter {
                keyword: "to",
                kind: kind::OBJECT,
                required: false,
            },
            Parameter {
                keyword: "from",
                kind: kind::OBJECT,
                required: true,
            },
            Parameter {
                keyword: "deep",
                kind: kind::BOOLEAN,
                required: false,
            },
        ]
    }

    fn examples(&self) -> &'static [Example] {
        &[Example {
            title: "merge objects",
            source: r#"merge({ "a": 1, "b": 2 }, { "b": 3, "c": 4 })"#,
            result: Ok(r#"{ "a": 1, "b": 3, "c": 4 }"#),
        }]
    }

    fn compile(
        &self,
        _state: &state::Compiler,
        _ctx: &FunctionCompileContext,
        mut arguments: ArgumentList,
    ) -> Compiled {
        let to = arguments.required("to");
        let from = arguments.required("from");
        let deep = arguments.optional("deep").unwrap_or_else(|| expr!(false));

        Ok(Box::new(MergeFn { to, from, deep }))
    }
}

#[derive(Debug, Clone)]
pub struct MergeFn {
    to: Box<dyn Expression>,
    from: Box<dyn Expression>,
    deep: Box<dyn Expression>,
}

impl Expression for MergeFn {
    fn resolve(&self, ctx: &mut Context) -> Resolved {
        let to_value = self.to.resolve(ctx)?;
        let returned = to_value.clone();
        let mut borrowed_to_value = to_value.borrow_mut();
        let mut borrowed_to_value = borrowed_to_value.as_object_mut().unwrap();

        let from_value = self.from.resolve(ctx)?;
        let from_value = from_value.borrow();
        let from_value = from_value.try_object()?;

        let deep = self.deep.resolve(ctx)?.try_boolean()?;

        merge_maps(&mut borrowed_to_value, &from_value, deep);

        Ok(returned)
    }

    fn type_def(&self, state: &state::Compiler) -> TypeDef {
        self.to
            .type_def(state)
            .merge_shallow(self.from.type_def(state))
    }
}

/// Merges two BTreeMaps of Symbol’s value as variable is void: Values. The
/// second map is merged into the first one.
///
/// If Symbol’s value as variable is void: deep is true, only the top level
/// values are merged in. If both maps contain a field with the same name, the
/// field from the first is overwritten with the field from the second.
///
/// If Symbol’s value as variable is void: deep is false, should both maps
/// contain a field with the same name, and both those fields are also maps, the
/// function will recurse and will merge the child fields from the second into
/// the child fields from the first.
///
/// Note, this does recurse, so there is the theoretical possibility that it
/// could blow up the stack. From quick tests on a sample project I was able to
/// merge maps with a depth of 3,500 before encountering issues. So I think that
/// is likely to be within acceptable limits. If it becomes a problem, we can
/// unroll this function, but that will come at a cost of extra code complexity.
fn merge_maps<K>(map1: &mut BTreeMap<K, SharedValue>, map2: &BTreeMap<K, SharedValue>, deep: bool)
where
    K: std::cmp::Ord + Clone,
{
    for (key2, value2) in map2.iter() {
        let value1 = map1.get_mut(key2);
        let borrowed2 = value2.borrow();
        match (deep, value1, &*borrowed2) {
            (true, Some(child1), Value::Object(ref child2)) => {
                let mut child1 = child1.borrow_mut();
                match &mut *child1 {
                    Value::Object(ref mut child1) => {
                        // We are doing a deep merge and both fields are maps.
                        merge_maps(child1, child2, deep);
                        continue;
                    }
                    _ => {}
                }
            }
            _ => {}
        }
        map1.insert(key2.clone(), value2.clone());
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use value::Kind;

    test_function! [
        merge => Merge;

        simple {
            args: func_args![
                to: value!({ key1: "val1" }),
                from: value!({ key2: "val2" })
            ],
            want: Ok(value!({ key1: "val1", key2: "val2" })),
            tdef: TypeDef::new().object::<String, TypeDef>(map! {
                "key1": Kind::Bytes,
                "key2": Kind::Bytes,
            }),
        }

        shallow {
            args: func_args![
                to: value!({
                    key1: "val1",
                    child: { grandchild1: "val1" },
                }),
                from: value!({
                    key2: "val2",
                    child: { grandchild2: true },
                })
            ],
            want: Ok(value!({
                key1: "val1",
                key2: "val2",
                child: { grandchild2: true },
            })),
            tdef: TypeDef::new().object::<String, TypeDef>(map! {
                "key1": Kind::Bytes,
                "key2": Kind::Bytes,
                "child": TypeDef::new().object::<String, TypeDef>(map! {
                    "grandchild2": Kind::Boolean,
                }),
            }),
        }

        deep {
            args: func_args![
                to: value!({
                    key1: "val1",
                    child: { grandchild1: "val1" },
                }),
                from: value!({
                    key2: "val2",
                    child: { grandchild2: true },
                }),
                deep: true,
            ],
            want: Ok(value!({
                key1: "val1",
                key2: "val2",
                child: {
                    grandchild1: "val1",
                    grandchild2: true,
                },
            })),
            tdef: TypeDef::new().object::<String, TypeDef>(map! {
                "key1": Kind::Bytes,
                "key2": Kind::Bytes,
                "child": TypeDef::new().object::<String, TypeDef>(map! {
                    "grandchild2": Kind::Boolean,
                }),
            }),

        }
    ];
}
