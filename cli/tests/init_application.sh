# The CLI binary. CI builds it once and exports FORKLAUNCH_CLI so the 43
# scripts share one compile instead of each re-checking a release build.
FL="${FORKLAUNCH_CLI:-cargo run --release}"
# KNOWN-ISSUE set-e exemption — this script tolerates in-test failures until
# #266: `forklaunch login` needs a live platform (device-code auth); fails in CI.
# Remove this exemption (restore `set -e`) when the issue is fixed.

echo "Processing applications..."
if [ -d "output/init-application" ]; then
    rm -rf output/init-application
fi

mkdir -p output/init-application
cd output/init-application

CACHE_FILE="processed_apps.cache"
touch "$CACHE_FILE"

RUST_BACKTRACE=1 $FL login

# databases=("postgresql" "mongodb" "sqlite" "mysql" "mssql" "libsql" "better-sqlite")
databases=("postgresql" "mongodb" "sqlite")
formatters=("prettier" "biome")
linters=("eslint" "oxlint")
validators=("zod" "typebox")
frameworks=("express" "hyper-express")
runtimes=("bun" "node")
test_frameworks=("vitest" "jest")
# licenses=("AGPL-3.0" "GPL-3.0" "LGPL-3.0" "Mozilla-2.0" "Apache-2.0" "MIT" "Boost-1.0" "Unlicense" "none")
licenses=("Mozilla-2.0")



for database in "${databases[@]}"; do
  for formatter in "${formatters[@]}"; do
    for linter in "${linters[@]}"; do
      for validator in "${validators[@]}"; do
        for framework in "${frameworks[@]}"; do
          for runtime in "${runtimes[@]}"; do
            if [ "$framework" = "hyper-express" ] && [ "$runtime" = "bun" ]; then
              continue
            fi
            for test_framework in "${test_frameworks[@]}"; do
              for license in "${licenses[@]}"; do
                license_name=$(echo "$license" | tr '[:upper:]' '[:lower:]' | cut -d'-' -f1)
                cache_name="application-${validator}-${framework}-${runtime}"
                app_name="${cache_name}-${database}-${linter}-${formatter}-${test_framework}-${license_name}"

                
                # Test current directory scenario
                RUST_BACKTRACE=1 $FL init application "$app_name" \
                    --path "./$app_name" \
                    -o "src/modules" \
                    -d "$database" \
                    -v "$validator" \
                    -f "$formatter" \
                    -l "$linter" \
                    -F "$framework" \
                    -r "$runtime" \
                    -t "$test_framework" \
                    -m "billing-base" \
                    -m "iam-base" \
                    -D "Test application" \
                    -A "Rohin Bhargava" \
                    -L "$license"

                if grep -Fxq "$cache_name" "$CACHE_FILE"; then
                    continue
                fi
                
                cd "$app_name/src/modules"
                if [ "$runtime" = "bun" ]; then
                  bun install
                else
                  pnpm install
                fi

                if [ "$runtime" = "bun" ]; then
                  bun run build
                else
                  pnpm build
                fi

                cd ../../..

                echo "$cache_name" >> "$CACHE_FILE"
              done
            done
          done
        done
      done
    done
  done
done
