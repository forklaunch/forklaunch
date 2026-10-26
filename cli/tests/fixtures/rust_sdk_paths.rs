#[cfg(test)]
mod path_parameter_conformance {
    use std::collections::BTreeMap;

    use super::path_with_params;

    #[test]
    fn encodes_utf8_and_reserved_bytes_without_replacing_parameter_prefixes_or_values() {
        let params = BTreeMap::from([
            ("id".into(), "a/b ?#%é🦀".into()),
            ("id2".into(), "second".into()),
            ("other".into(), ":id2/{id}".into()),
            ("literal".into(), "ignored".into()),
        ]);
        assert_eq!(
            path_with_params("/users/:id/:id2/{other}/literal:id/{missing}", &params),
            "/users/a%2Fb%20%3F%23%25%C3%A9%F0%9F%A6%80/second/%3Aid2%2F%7Bid%7D/literal:id/{missing}"
        );
        assert_eq!(
            path_with_params("/:id/{id}", &params),
            "/a%2Fb%20%3F%23%25%C3%A9%F0%9F%A6%80/a%2Fb%20%3F%23%25%C3%A9%F0%9F%A6%80"
        );
        let params = BTreeMap::from([("id".into(), "AZaz09-_.!~*'()".into())]);
        assert_eq!(path_with_params("/:id", &params), "/AZaz09-_.!~*'()");
    }
}
