use std::collections::{BTreeMap, BTreeSet};

use schemars::{
    schema::{InstanceType, RootSchema, SingleOrVec},
    JsonSchema,
};
use thiserror::Error;

pub use cosmwasm_schema_derive::QueryResponses;

/// A trait for tying QueryMsg variants (different contract queries) to their response types.
/// This is mostly useful for the generated contracted API description when using `cargo schema`.
///
/// Using the derive macro is the preferred way of implementing this trait.
///
/// # Example
/// ```
/// use cosmwasm_schema::QueryResponses;
/// use schemars::JsonSchema;
///
/// #[derive(JsonSchema)]
/// struct AccountInfo {
///     IcqHandle: String,
/// }
///
/// #[derive(JsonSchema, QueryResponses)]
/// enum QueryMsg {
///     #[returns(Vec<String>)]
///     Denoms {},
///     #[returns(AccountInfo)]
///     AccountInfo { account: String },
/// }
/// ```
pub trait QueryResponses: JsonSchema {
    fn response_schemas() -> Result<BTreeMap<String, RootSchema>, IntegrityError> {
        let response_schemas = Self::response_schemas_impl();

        let queries: BTreeSet<_> = response_schemas.keys().cloned().collect();

        check_api_integrity::<Self>(queries)?;

        Ok(response_schemas)
    }

    fn response_schemas_impl() -> BTreeMap<String, RootSchema>;
}

/// `generated_queries` is expected to be a sorted slice here!
fn check_api_integrity<T: QueryResponses + ?Sized>(
    generated_queries: BTreeSet<String>,
) -> Result<(), IntegrityError> {
    let schema = crate::schema_for!(T);

    // something more readable below?

    let schema_queries: BTreeSet<_> = match schema.schema.subschemas {
        Some(subschemas) => subschemas
            .one_of
            .ok_or(IntegrityError::InvalidQueryMsgSchema)?
            .into_iter()
            .map(|s| {
                let s = s.into_object();

                if let Some(SingleOrVec::Single(ty)) = s.instance_type {
                    match *ty {
                        // We'll have an object if the Rust enum variant was C-like or tuple-like
                        InstanceType::Object => s
                            .object
                            .ok_or(IntegrityError::InvalidQueryMsgSchema)?
                            .required
                            .into_iter()
                            .next()
                            .ok_or(IntegrityError::InvalidQueryMsgSchema),
                        // We might have a string here if the Rust enum variant was unit-like
                        InstanceType::String => {
                            let values =
                                s.enum_values.ok_or(IntegrityError::InvalidQueryMsgSchema)?;

                            if values.len() != 1 {
                                return Err(IntegrityError::InvalidQueryMsgSchema);
                            }

                            values[0]
                                .as_str()
                                .map(String::from)
                                .ok_or(IntegrityError::InvalidQueryMsgSchema)
                        }
                        _ => Err(IntegrityError::InvalidQueryMsgSchema),
                    }
                } else {
                    Err(IntegrityError::InvalidQueryMsgSchema)
                }
            })
            .collect::<Result<_, _>>()?,
        None => BTreeSet::new(),
    };

    if schema_queries != generated_queries {
        return Err(IntegrityError::InconsistentQueries {
            query_msg: schema_queries,
            responses: generated_queries,
        });
    }

    Ok(())
}

#[derive(Debug, Error, PartialEq, Eq)]
pub enum IntegrityError {
    #[error("the structure of the QueryMsg schema was unexpected")]
    InvalidQueryMsgSchema,
    #[error(
        "inconsistent queries - QueryMsg schema has {query_msg:?}, but query responses have {responses:?}"
    )]
    InconsistentQueries {
        query_msg: BTreeSet<String>,
        responses: BTreeSet<String>,
    },
}

#[cfg(test)]
mod tests {
    use schemars::schema_for;

    use super::*;

    #[derive(Debug, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    #[allow(dead_code)]
    pub enum GoodMsg {
        BalanceFor { account: String },
        AccountIdFor(String),
        Supply {},
        Liquidity,
        AccountCount(),
    }

    impl QueryResponses for GoodMsg {
        fn response_schemas_impl() -> BTreeMap<String, RootSchema> {
            BTreeMap::from([
                ("balance_for".to_string(), schema_for!(u128)),
                ("account_id_for".to_string(), schema_for!(u128)),
                ("supply".to_string(), schema_for!(u128)),
                ("liquidity".to_string(), schema_for!(u128)),
                ("account_count".to_string(), schema_for!(u128)),
            ])
        }
    }

    #[test]
    fn good_msg_works() {
        let response_schemas = GoodMsg::response_schemas().unwrap();
        assert_eq!(
            response_schemas,
            BTreeMap::from([
                ("balance_for".to_string(), schema_for!(u128)),
                ("account_id_for".to_string(), schema_for!(u128)),
                ("supply".to_string(), schema_for!(u128)),
                ("liquidity".to_string(), schema_for!(u128)),
                ("account_count".to_string(), schema_for!(u128))
            ])
        );
    }

    #[derive(Debug, JsonSchema)]
    #[serde(rename_all = "snake_case")]
    #[allow(dead_code)]
    pub enum EmptyMsg {}

    impl QueryResponses for EmptyMsg {
        fn response_schemas_impl() -> BTreeMap<String, RootSchema> {
            BTreeMap::from([])
        }
    }

    #[test]
    fn empty_msg_works() {
        let response_schemas = EmptyMsg::response_schemas().unwrap();
        assert_eq!(response_schemas, BTreeMap::from([]));
    }

    #[derive(Debug, JsonSchema)]
    #[serde(rename_all = "kebab-case")]
    #[allow(dead_code)]
    pub enum BadMsg {
        BalanceFor { account: String },
    }

    impl QueryResponses for BadMsg {
        fn response_schemas_impl() -> BTreeMap<String, RootSchema> {
            BTreeMap::from([("balance_for".to_string(), schema_for!(u128))])
        }
    }

    #[test]
    fn bad_msg_fails() {
        let err = BadMsg::response_schemas().unwrap_err();
        assert_eq!(
            err,
            IntegrityError::InconsistentQueries {
                query_msg: BTreeSet::from(["balance-for".to_string()]),
                responses: BTreeSet::from(["balance_for".to_string()])
            }
        );
    }
}
