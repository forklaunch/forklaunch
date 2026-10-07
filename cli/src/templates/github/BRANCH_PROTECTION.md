# Branch Protection Setup Guide

This guide explains how to enable branch protection rules for your GitHub repository to enforce code quality and review standards.

## Steps to Enable Branch Protection

1. Navigate to your repository on GitHub.
2. Go to **Settings** > **Branches**.
3. Under **Branch protection rules**, click **Add rule**.
4. In the **Branch name pattern** field, enter `main`.

## Recommended Settings

### Require Pull Request Reviews

- Check **Require a pull request before merging**.
- Set **Required number of approvals before merging** to at least `1`.
- Optionally enable **Dismiss stale pull request approvals when new commits are pushed**.

### Require Status Checks to Pass

- Check **Require status checks to pass before merging**.
- Search for and select the **ci** status check (from the CI workflow in `.github/workflows/ci.yml`).
- Check **Require branches to be up to date before merging** to ensure the branch is tested with the latest base branch changes.

### Additional Recommendations

- **Require conversation resolution before merging** to ensure all review comments are addressed.
- **Do not allow bypassing the above settings** to enforce rules for all contributors, including administrators.
- **Restrict who can push to matching branches** if you want to limit direct pushes.

## Applying the Rule

Click **Create** (or **Save changes** if editing an existing rule) to apply the branch protection settings.
