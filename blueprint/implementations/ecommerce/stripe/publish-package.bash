#!/bin/bash

# Automatically fetch the package name from package.json
PACKAGE_NAME=$(node -p "require('./package.json').name")

# Check current version of the package
CURRENT_VERSION=$(node -p "require('./package.json').version")
LAST_PUBLISHED_VERSION=$(npm show "$PACKAGE_NAME" version)

# Check if the version has changed
if [ "$CURRENT_VERSION" != "$LAST_PUBLISHED_VERSION" ]; then
  echo "Publishing new version of $PACKAGE_NAME..."
  pnpm publish "$@"
else
  echo "Version has not changed for $PACKAGE_NAME, skipping publish."
fi


