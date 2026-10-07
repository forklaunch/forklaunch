# Define arrays of directories and packages
DIRS=("billing" "iam" "messaging" "worker")
PACKAGES=("common" "validator" "core" "express" "hyper-express" "universal-sdk")

# Loop through each directory
for dir in "${DIRS[@]}"; do
  cd $dir

  # Loop through each package and create symlink
  for package in "${PACKAGES[@]}"; do
    pnpm link ../../../framework/$package
  done

  cd ..
done
