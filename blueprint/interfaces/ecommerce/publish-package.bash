#!/bin/bash

# Automatically fetch the package name from package.json
PACKAGE_NAME=$(node -p "require('./package.json').name")

# Check current version of the package
CURRENT_VERSION=$(node -p "require('./package.json').version")

# A package that has never been published makes `npm show` exit non-zero and
# print a multi-line "npm error code E404" block. That is the ordinary
# first-publish case rather than a failure, and the noise reads exactly like a
# fatal error, so the lookup is silenced and an empty result is handled below.
LAST_PUBLISHED_VERSION=$(npm show "$PACKAGE_NAME" version 2>/dev/null)

if [ -z "$LAST_PUBLISHED_VERSION" ]; then
  echo "$PACKAGE_NAME has never been published, publishing $CURRENT_VERSION..."
  pnpm publish "$@"
elif [ "$CURRENT_VERSION" != "$LAST_PUBLISHED_VERSION" ]; then
  echo "Publishing new version of $PACKAGE_NAME..."
  pnpm publish "$@"
else
  echo "Version has not changed for $PACKAGE_NAME, skipping publish."
fi
