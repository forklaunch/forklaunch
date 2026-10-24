//! Opt-in static-segment matching with pinned ECMAScript non-Unicode `/i` semantics.
use super::{Next, Parts, Request, Response, Router, RouterConfig};
use forklaunch_core::contract::HttpMethod;

#[derive(Clone)]
struct OriginalRoute {
    uri: axum::http::Uri,
    method: axum::http::Method,
    dispatch_path: String,
    selected_method: Option<HttpMethod>,
}

#[derive(Clone)]
pub(super) struct OptionsAllow(pub(super) String);

pub(super) fn dispatch_path(parts: &Parts) -> Option<&str> {
    parts
        .extensions
        .get::<OriginalRoute>()
        .map(|value| value.dispatch_path.as_str())
}

pub(super) fn selected_method(extensions: &axum::http::Extensions) -> Option<HttpMethod> {
    extensions
        .get::<OriginalRoute>()
        .and_then(|value| value.selected_method)
}

pub(super) async fn restore(mut request: Request, next: Next) -> Response {
    if let Some(original) = request.extensions().get::<OriginalRoute>() {
        let uri = original.uri.clone();
        let method = original.method.clone();
        *request.uri_mut() = uri;
        *request.method_mut() = method;
    }
    next.run(request).await
}

/// Remove a known number of mount segments without assuming equal UTF-8 lengths.
pub(super) fn local_path(path: &str, prefix: &str) -> String {
    let count = prefix
        .split('/')
        .filter(|segment| !segment.is_empty())
        .count();
    let mut offset = 0;
    for _ in 0..count {
        let start = offset + usize::from(path.as_bytes().get(offset) == Some(&b'/'));
        offset = path[start..]
            .find('/')
            .map_or(path.len(), |index| start + index);
    }
    forklaunch_common::normalize_route_path(&path[offset..])
}

#[cfg(not(feature = "typescript-routing"))]
pub(super) fn wrap(
    router: Router,
    _isolated: std::collections::HashMap<String, Router>,
    _registry: &forklaunch_core::contract::ContractRegistry,
    config: &RouterConfig,
    _protected: Vec<String>,
) -> Router {
    assert!(
        config.typescript_case_insensitive_paths.is_empty(),
        "case-insensitive routing requires typescript-routing feature"
    );
    router
}

#[cfg(feature = "typescript-routing")]
mod enabled {
    use super::{
        HttpMethod, Next, OptionsAllow, OriginalRoute, Request, Response, Router, RouterConfig,
    };
    use axum::{
        extract::State,
        http::{Method, StatusCode},
        middleware,
        response::IntoResponse,
    };
    use forklaunch_common::zod_regex::{RegexCompileLimits, RegexMatchLimits, ZodRegex};
    use std::{collections::HashMap, sync::Arc};
    use tower::ServiceExt;

    enum Segment {
        Parameter,
        CatchAll,
        Literal(String, Option<ZodRegex>),
    }
    struct Pattern {
        path: String,
        segments: Vec<Segment>,
        trailing: bool,
        selected: bool,
        options: bool,
        router: Router,
    }
    struct Entry {
        pattern: usize,
        method: HttpMethod,
    }
    type Choice = (usize, Option<HttpMethod>, String, Option<String>);

    struct Dispatch {
        patterns: Vec<Pattern>,
        entries: Vec<Entry>,
        protected: Vec<String>,
    }

    fn literal_regex(value: &str) -> ZodRegex {
        let mut pattern = String::from("^(?:");
        for character in value.chars() {
            if "^$\\.*+?()[]{}|".contains(character) {
                pattern.push('\\');
            }
            pattern.push(character);
        }
        pattern.push_str(")$");
        ZodRegex::compile(
            &pattern.encode_utf16().collect::<Vec<_>>(),
            "i",
            RegexCompileLimits::default(),
        )
        .expect("case routing literal exceeds regex admission limits")
    }

    #[test]
    fn pinned_case_literal_matches_ecmascript_non_unicode_observations() {
        for (literal, candidate, expected) in [
            ("café", "CAFÉ", true),
            ("Σ", "ς", true),
            ("s", "ſ", false),
            ("k", "\u{212a}", false),
            ("i", "İ", false),
            ("ß", "SS", false),
            ("𐐀", "𐐨", false),
            ("%c3%a9", "%C3%A9", true),
            ("café", "caf%C3%A9", false),
            ("payment-link", "%70ayment-link", false),
        ] {
            assert_eq!(
                literal_regex(literal)
                    .test(candidate, RegexMatchLimits::default())
                    .unwrap(),
                expected,
                "{literal} / {candidate}"
            );
        }
    }

    impl Pattern {
        fn new(path: &str, config: &RouterConfig, router: Router) -> Self {
            let selected = config.typescript_case_insensitive_paths.contains(path);
            let segments = if path == "/" {
                Vec::new()
            } else {
                path.trim_start_matches('/')
                    .split('/')
                    .map(|part| {
                        if part.starts_with('{') && part.ends_with('}') {
                            if part.starts_with("{*") {
                                Segment::CatchAll
                            } else {
                                Segment::Parameter
                            }
                        } else {
                            Segment::Literal(part.to_owned(), selected.then(|| literal_regex(part)))
                        }
                    })
                    .collect()
            };
            Self {
                path: path.to_owned(),
                segments,
                selected,
                router,
                trailing: config.optional_trailing_slash_paths.contains(path),
                options: config.typescript_options_paths.contains(path),
            }
        }

        fn rewrite(&self, raw: &str) -> Result<Option<String>, ()> {
            let trailing = raw != "/" && raw.ends_with('/');
            let catch_all = matches!(self.segments.last(), Some(Segment::CatchAll));
            if trailing && !self.trailing && !catch_all {
                return Ok(None);
            }
            let optional_slash = trailing && self.trailing;
            let candidate = if optional_slash {
                &raw[..raw.len() - 1]
            } else {
                raw
            };
            let segments: Vec<_> = if candidate == "/" {
                Vec::new()
            } else {
                candidate
                    .strip_prefix('/')
                    .unwrap_or(candidate)
                    .split('/')
                    .collect()
            };
            if (catch_all && segments.len() < self.segments.len())
                || (!catch_all && segments.len() != self.segments.len())
            {
                return Ok(None);
            }
            let mut output = Vec::with_capacity(segments.len());
            for (index, (part, segment)) in segments.iter().copied().zip(&self.segments).enumerate()
            {
                match segment {
                    Segment::CatchAll => {
                        if segments[index..].join("/").is_empty() {
                            return Ok(None);
                        }
                        output.extend_from_slice(&segments[index..]);
                        break;
                    }
                    Segment::Parameter if !part.is_empty() => output.push(part),
                    Segment::Literal(canonical, regex) => {
                        let matches = if let Some(regex) = regex {
                            regex
                                .test(part, RegexMatchLimits::default())
                                .map_err(|_| ())?
                        } else {
                            canonical == part
                        };
                        if !matches {
                            return Ok(None);
                        }
                        output.push(canonical);
                    }
                    Segment::Parameter => return Ok(None),
                }
            }
            let mut path = format!("/{}", output.join("/"));
            if optional_slash {
                path.push('/');
            }
            Ok(Some(path))
        }
    }

    pub(crate) fn install(
        router: Router,
        mut isolated: HashMap<String, Router>,
        registry: &forklaunch_core::contract::ContractRegistry,
        config: &RouterConfig,
        protected: Vec<String>,
    ) -> Router {
        if config.typescript_case_insensitive_paths.is_empty() {
            return router;
        }
        let mut patterns = Vec::<Pattern>::new();
        let mut entries = Vec::new();
        for route in registry.routes() {
            let index = if let Some(index) = patterns
                .iter()
                .position(|pattern| pattern.path == route.openapi_path)
            {
                index
            } else {
                let isolated = isolated
                    .remove(&route.openapi_path)
                    .expect("missing isolated route");
                patterns.push(Pattern::new(&route.openapi_path, config, isolated));
                patterns.len() - 1
            };
            entries.push(Entry {
                pattern: index,
                method: route.method,
            });
        }
        assert!(
            config
                .typescript_case_insensitive_paths
                .iter()
                .all(|path| patterns.iter().any(|pattern| &pattern.path == path)),
            "unknown case-insensitive route selection"
        );
        let dispatch = Arc::new(Dispatch {
            patterns,
            entries,
            protected,
        });
        Router::new()
            .fallback_service(router)
            .layer(middleware::from_fn_with_state(dispatch, rewrite))
    }

    impl Dispatch {
        fn select(&self, request: &Request) -> Result<Option<Choice>, ()> {
            let raw = request.uri().path();
            if self
                .protected
                .iter()
                .any(|prefix| super::super::internal::contains_path(prefix, raw))
            {
                return Ok(None);
            }
            let matched = self
                .patterns
                .iter()
                .map(|pattern| pattern.rewrite(raw))
                .collect::<Result<Vec<_>, _>>()?;
            if !self
                .patterns
                .iter()
                .zip(&matched)
                .any(|(pattern, path)| pattern.selected && path.is_some())
            {
                return Ok(None);
            }
            for entry in &self.entries {
                if let Some(path) = &matched[entry.pattern]
                    && (request.method().as_str() == entry.method.as_str()
                        || request.method() == Method::HEAD && entry.method == HttpMethod::Get)
                {
                    return Ok(Some((
                        entry.pattern,
                        Some(entry.method),
                        path.clone(),
                        None,
                    )));
                }
            }
            if request.method() == Method::OPTIONS
                && let Some((index, path)) = matched.iter().enumerate().find_map(|(index, path)| {
                    path.as_ref()
                        .filter(|_| self.patterns[index].options)
                        .map(|path| (index, path))
                })
            {
                let mut methods = self
                    .entries
                    .iter()
                    .filter(|entry| matched[entry.pattern].is_some())
                    .map(|entry| entry.method.as_str())
                    .collect::<Vec<_>>();
                if methods.contains(&"GET") {
                    methods.push("HEAD");
                }
                methods.sort_unstable();
                methods.dedup();
                return Ok(Some((
                    index,
                    Some(HttpMethod::Options),
                    path.clone(),
                    Some(methods.join(", ")),
                )));
            }
            // Preserve historical native unsupported-method handling in this profile.
            Ok(matched.iter().enumerate().find_map(|(index, path)| {
                path.as_ref().map(|path| (index, None, path.clone(), None))
            }))
        }
    }

    fn prepare(
        request: &mut Request,
        method: Option<HttpMethod>,
        path: &str,
        allow: Option<String>,
    ) -> Result<(), ()> {
        let original = OriginalRoute {
            uri: request.uri().clone(),
            method: request.method().clone(),
            dispatch_path: path.to_owned(),
            selected_method: method,
        };
        let query = request
            .uri()
            .query()
            .map_or_else(String::new, |query| format!("?{query}"));
        let mut parts = request.uri().clone().into_parts();
        parts.path_and_query = Some(format!("{path}{query}").parse().map_err(|_| ())?);
        let uri = axum::http::Uri::from_parts(parts).map_err(|_| ())?;
        let method = method
            .map(|method| Method::from_bytes(method.as_str().as_bytes()))
            .transpose()
            .map_err(|_| ())?;
        if request
            .extensions()
            .get::<axum::extract::OriginalUri>()
            .is_none()
        {
            request
                .extensions_mut()
                .insert(axum::extract::OriginalUri(original.uri.clone()));
        }
        if let Some(allow) = allow {
            request.extensions_mut().insert(OptionsAllow(allow));
        }
        request.extensions_mut().insert(original);
        *request.uri_mut() = uri;
        if let Some(method) = method {
            *request.method_mut() = method;
        }
        Ok(())
    }

    async fn rewrite(
        State(dispatch): State<Arc<Dispatch>>,
        mut request: Request,
        next: Next,
    ) -> Response {
        match dispatch.select(&request) {
            Ok(Some((index, method, path, allow))) => {
                let head = request.method() == Method::HEAD;
                if prepare(&mut request, method, &path, allow).is_err() {
                    return StatusCode::INTERNAL_SERVER_ERROR.into_response();
                }
                match dispatch.patterns[index]
                    .router
                    .clone()
                    .oneshot(request)
                    .await
                {
                    Ok(mut response) => {
                        if head {
                            *response.body_mut() = axum::body::Body::empty();
                        }
                        response
                    }
                    Err(never) => match never {},
                }
            }
            Ok(None) => next.run(request).await,
            Err(()) => StatusCode::INTERNAL_SERVER_ERROR.into_response(),
        }
    }
}

#[cfg(feature = "typescript-routing")]
pub(super) use enabled::install as wrap;

#[cfg(all(test, feature = "typescript-routing"))]
mod tests;

#[cfg(all(test, not(feature = "typescript-routing")))]
#[test]
#[should_panic(expected = "case-insensitive routing requires typescript-routing feature")]
fn selected_case_profile_without_feature_fails_closed() {
    let _ = super::ForkLaunchRouter::anonymous()
        .get(
            forklaunch_core::contract::RouteContract::get("Read", "Read", "/read"),
            || async { "ok" },
        )
        .with_config(RouterConfig::default().with_typescript_case_insensitive_path("/read"))
        .into_router();
}
