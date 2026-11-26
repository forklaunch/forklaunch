//! Internal Zod issue shape, preserved until the public adapter formats it.
use super::{Budget, ParseError, ZodJsonUnsupported};

pub(super) enum Issue {
    Leaf(ParseError),
    /// Ordering is needed only if this diagnostic survives union selection.
    UnorderedObject,
    Union {
        path: Vec<String>,
        branches: Vec<Vec<Self>>,
    },
}
impl Issue {
    pub fn prepend(&mut self, key: &str, budget: &mut Budget) -> Result<(), ZodJsonUnsupported> {
        budget.charge(1, key.len())?;
        match self {
            Self::Leaf(error) => error.path.insert(0, key.to_owned()),
            Self::UnorderedObject => {}
            Self::Union { path, branches } => {
                path.insert(0, key.to_owned());
                for issue in branches.iter_mut().flatten() {
                    issue.prepend(key, budget)?;
                }
            }
        }
        Ok(())
    }
}

pub(super) fn format(
    issues: Vec<Issue>,
    budget: &mut Budget,
) -> Result<Vec<ParseError>, ZodJsonUnsupported> {
    let mut output = Vec::new();
    for issue in issues {
        match issue {
            Issue::Leaf(error) => output.push(error),
            Issue::UnorderedObject => return Err(ZodJsonUnsupported::ObjectOrderUnavailable),
            Issue::Union { path, branches } => {
                for (index, branch) in branches.into_iter().enumerate() {
                    for issue in branch {
                        // ForkLaunch's adapter flattens exactly one union level.
                        // Branch issue paths already include the union's outer
                        // path; the adapter deliberately prepends it again.
                        let outer_bytes = path
                            .iter()
                            .fold(64usize, |size, part| size.saturating_add(part.len()));
                        budget.charge(1 + path.len(), outer_bytes)?;
                        let mut projected = Vec::with_capacity(1 + path.len());
                        projected.push(format!("Union Schema Variant {index}"));
                        projected.extend(path.iter().cloned());
                        let (child_path, message) = match issue {
                            Issue::Leaf(error) => (error.path, error.message),
                            Issue::UnorderedObject => {
                                return Err(ZodJsonUnsupported::ObjectOrderUnavailable);
                            }
                            Issue::Union { path, .. } => {
                                budget.charge(0, 13)?;
                                (path, "Invalid input".into())
                            }
                        };
                        projected.extend(child_path);
                        output.push(ParseError {
                            path: projected,
                            message,
                        });
                    }
                }
            }
        }
    }
    Ok(output)
}
