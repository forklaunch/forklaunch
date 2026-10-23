//! Query-string request mappers. Values follow the SDK/MCP JSON-composite wire
//! representation; a malformed or repeated field is `ecommerce_invalid_query`.
use forklaunch_core::domain::BlueprintError;
use forklaunch_core::domain::BlueprintResult;
use forklaunch_ecommerce_interface::{ProductSearchDto, catalog::CatalogQuery};
use std::collections::BTreeMap;
fn invalid() -> BlueprintError {
    BlueprintError::new("ecommerce_invalid_query", "Invalid ecommerce query")
}
fn fields(query: Option<&str>) -> BlueprintResult<BTreeMap<String, String>> {
    let mut result = BTreeMap::new();
    for (name, value) in form_urlencoded::parse(query.unwrap_or_default().as_bytes()).into_owned() {
        if result.insert(name, value).is_some() {
            return Err(invalid());
        }
    }
    Ok(result)
}
fn array(fields: &BTreeMap<String, String>) -> BlueprintResult<Option<Vec<String>>> {
    fields
        .get("ids")
        .map(|value| serde_json::from_str(value).map_err(|_| invalid()))
        .transpose()
}
fn integer(fields: &BTreeMap<String, String>, name: &str) -> BlueprintResult<Option<i64>> {
    fields
        .get(name)
        .map(|value| value.parse().map_err(|_| invalid()))
        .transpose()
}
/// The optional JSON `ids` array of a list query.
///
/// # Errors
/// Rejects repeated fields and a malformed `ids` value.
pub fn ids(query: Option<&str>) -> BlueprintResult<Option<Vec<String>>> {
    array(&fields(query)?)
}
/// The product search filters.
///
/// # Errors
/// Rejects repeated fields and malformed values.
pub fn search(query: Option<&str>) -> BlueprintResult<ProductSearchDto> {
    let fields = fields(query)?;
    Ok(ProductSearchDto {
        ids: array(&fields)?,
        title: fields.get("title").cloned(),
        min_price_cents: integer(&fields, "minPriceCents")?,
        max_price_cents: integer(&fields, "maxPriceCents")?,
        in_stock: fields
            .get("inStock")
            .map(|value| value.parse().map_err(|_| invalid()))
            .transpose()?,
        option_name: fields.get("optionName").cloned(),
        option_value: fields.get("optionValue").cloned(),
    })
}
/// The public catalog filters and page.
///
/// # Errors
/// Rejects repeated fields and malformed values.
pub fn catalog(query: Option<&str>) -> BlueprintResult<CatalogQuery> {
    let fields = fields(query)?;
    Ok(CatalogQuery {
        ids: array(&fields)?,
        title: fields.get("title").cloned(),
        limit: integer(&fields, "limit")?,
        offset: integer(&fields, "offset")?,
    })
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn typed_queries_preserve_composites_false_empty_and_native_integer_bounds() {
        let query = form_urlencoded::Serializer::new(String::new())
            .append_pair("ids", r#"["a,b","水"]"#)
            .append_pair("inStock", "false")
            .append_pair("minPriceCents", "0")
            .append_pair("title", "a+b & 水")
            .finish();
        let search = search(Some(&query)).unwrap();
        assert_eq!(search.ids, Some(vec!["a,b".into(), "水".into()]));
        assert_eq!(search.in_stock, Some(false));
        assert_eq!(search.min_price_cents, Some(0));
        assert_eq!(search.title.as_deref(), Some("a+b & 水"));
        assert_eq!(ids(Some("ids=%5B%5D")).unwrap(), Some(Vec::new()));
        assert_eq!(ids(None).unwrap(), None);
        for query in [
            "ids=a,b",
            "ids=null",
            "ids=%5B1%5D",
            "inStock=yes",
            "minPriceCents=1.5",
            "minPriceCents=9223372036854775808",
            "inStock=true&inStock=false",
        ] {
            assert!(super::search(Some(query)).is_err(), "{query}");
        }
        for query in ["limit=1.5", "offset=NaN", "limit=1&limit=2"] {
            assert!(catalog(Some(query)).is_err());
        }
        let page = catalog(Some("limit=-10&offset=-3")).unwrap();
        assert_eq!(page.limit, Some(-10));
        assert_eq!(page.offset, Some(-3));
    }
}
