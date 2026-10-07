# Define arrays of directories and packages
DIRS=("core" "billing" "iam" "messaging-base" "messaging-twilio" "monitoring" "sample-worker")
PACKAGES=("common" "validator" "core" "express" "hyper-express" "universal-sdk")

# Loop through each directory
for dir in "${DIRS[@]}"; do
  cd $dir

  # Loop through each package and create symlink
  for package in "${PACKAGES[@]}"; do
    pnpm link ../../framework/$package
  done

  cd ..
done

cd interfaces

./link.sh

cd ../implementations

./link.sh