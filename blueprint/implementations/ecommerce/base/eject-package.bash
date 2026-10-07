rm -rf lib/eject
mkdir -p lib/eject/domain

cp -r domain/schemas/zod lib/eject/domain/schemas
cp domain/schemas/index.ts lib/eject/domain/schemas
find lib/eject/domain/schemas -type f -name "*.ts" -exec perl -i -pe "s|'\.\.\/\.\.|'\.\.|g" {} \;

if [ -d "services" ]; then
  cp -r services lib/eject/services
fi

if [ -d "domain/types" ]; then
  cp -r domain/types lib/eject/domain/types
fi

if [ -d "domain/interfaces" ]; then
  cp -r domain/interfaces lib/eject/domain/interfaces
fi

if [ -d "consumers" ]; then
  cp -r consumers lib/eject/consumers
fi

if [ -d "producers" ]; then
  cp -r producers lib/eject/producers
fi

if [ -d "domain/enum" ]; then
  cp -r domain/enum lib/eject/domain/enum
fi

find lib/eject -type f -name '*.ts' -exec sh -c \
  'for f do \
    sed "s|@forklaunch/validator/zod|@{{app_name}}/core|g" "$f" > "$f.tmp" && \
    mv "$f.tmp" "$f"; \
  done' sh {} +