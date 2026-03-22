/* eslint-disable @typescript-eslint/no-unused-vars */
import { Schema } from '@forklaunch/validator';
import { SchemaValidator, number, string } from '@forklaunch/validator/typebox';
import { EntityManager, InferEntity, defineEntity, p } from '@mikro-orm/core';
import { requestMapper, responseMapper } from '../src/mappers';

const SV = SchemaValidator();

const TestEntity = defineEntity({
  name: 'TestEntity',
  properties: {
    id: p.uuid(),
    createdAt: p.datetime(),
    updatedAt: p.datetime(),
    name: p.string(),
    age: p.integer()
  }
});

type TestEntityType = InferEntity<typeof TestEntity>;

const TestSchema = {
  id: string,
  name: string,
  age: number
};

const TestRequestMapper = requestMapper({
  schemaValidator: SV,
  schema: TestSchema,
  entity: TestEntity,
  mapperDefinition: {
    toEntity: async (dto, em: EntityManager) => {
      return em.create(TestEntity, {
        id: dto.id,
        name: dto.name,
        age: dto.age,
        createdAt: new Date(),
        updatedAt: new Date()
      });
    }
  }
});

const TestResponseMapper = responseMapper({
  schemaValidator: SV,
  schema: TestSchema,
  entity: TestEntity,
  mapperDefinition: {
    toDto: async (entity) => {
      return {
        id: entity.id,
        name: entity.name,
        age: entity.age
      };
    }
  }
});

describe('request mappers tests', () => {
  test('schema static and constructed equality', async () => {
    expect(TestRequestMapper.schema).toEqual(TestSchema);
  });

  test('deserialize failure', async () => {
    const json = {
      id: '123',
      name: 'test'
    };

    await expect(
      async () =>
        await TestRequestMapper.toEntity(
          // @ts-expect-error - missing age
          json,
          {} as EntityManager
        )
    ).rejects.toThrow();
  });
});

describe('response mappers tests', () => {
  test('schema static and constructed equality', async () => {
    expect(TestResponseMapper.schema).toEqual(TestSchema);
  });

  test('serialize', async () => {
    const entity: TestEntityType = {
      id: '123',
      name: 'test',
      age: 1,
      createdAt: new Date(0),
      updatedAt: new Date(0)
    };

    const result = await TestResponseMapper.toDto(entity);
    const expectedDto = {
      id: '123',
      name: 'test',
      age: 1
    };

    expect(result).toEqual(expectedDto);
  });

  test('serialize failure', async () => {
    const entity = {
      id: '123',
      name: 'test',
      createdAt: new Date(),
      updatedAt: new Date()
    } as TestEntityType;

    await expect(
      async () => await TestResponseMapper.toDto(entity)
    ).rejects.toThrow();
  });
});
