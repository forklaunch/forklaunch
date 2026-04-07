import { openApiCompliantPath } from '@forklaunch/common';
import {
  AnySchemaValidator,
  IdiomaticSchema,
  SchemaValidator
} from '@forklaunch/validator';
import {
  ContentObject,
  OpenAPIObject,
  OperationObject,
  PathObject,
  ResponseObject,
  ResponsesObject,
  SecuritySchemeObject,
  TagObject
} from 'openapi3-ts/oas31';
import HTTPStatuses from '../httpStatusCodes';
import {
  discriminateBody,
  discriminateResponseBodies
} from '../router/discriminateBody';
import { unpackRouters } from '../router/unpackRouters';
import {
  Body,
  HeadersObject,
  Method,
  ParamsObject,
  QueryObject,
  ResponseBody,
  SchemaAuthMethods,
  SessionObject,
  VersionSchema
} from '../types/contractDetails.types';
import { ExpressLikeApplicationOptions } from '../types/expressLikeOptions';
import { ForklaunchRouter } from '../types/router.types';

export const OPENAPI_DEFAULT_VERSION = Symbol('default');

/**
 * Capitalizes the first letter of a string.
 *
 * @param {string} str - The string to transform.
 * @returns {string} - The transformed string with the first letter capitalized.
 */
function toUpperCase(str: string) {
  return str.charAt(0).toUpperCase() + str.slice(1);
}

/**
 * Transforms a base path by ensuring it starts with a slash and capitalizing the first letter.
 *
 * @param {string} basePath - The base path to transform.
 * @returns {string} - The transformed base path.
 */
function transformBasePath(basePath: string) {
  if (basePath.startsWith('/')) {
    return basePath.slice(1);
  }
  return `/${basePath}`;
}

/**
 * Creates a Swagger document.
 *
 * @param {string | number} port - The port on which the server is running.
 * @param {TagObject[]} tags - The tags for the Swagger document.
 * @param {PathObject} paths - The paths for the Swagger document.
 * @returns {OpenAPIObject} - The Swagger document.
 */
function generateOpenApiDocument<SV extends AnySchemaValidator>(
  serverUrls: string[],
  serverDescriptions: string[],
  versionedTags: Record<string | symbol, TagObject[]>,
  versionedPaths: Record<string | symbol, PathObject>,
  versionedSecuritySchemes: Record<
    string | symbol,
    Record<string, SecuritySchemeObject>
  >,
  appOptions?: ExpressLikeApplicationOptions<SV, SessionObject<SV>>['openapi']
): Record<string | symbol, OpenAPIObject> {
  const { title, description, contact } =
    appOptions !== false ? (appOptions ?? {}) : {};
  return {
    [OPENAPI_DEFAULT_VERSION]: {
      openapi: '3.1.0',
      info: {
        title: title || process.env.API_TITLE || 'API Definition',
        version: 'latest',
        description,
        contact
      },
      components: {
        securitySchemes: versionedSecuritySchemes[OPENAPI_DEFAULT_VERSION]
      },
      tags: versionedTags[OPENAPI_DEFAULT_VERSION],
      servers: [
        ...serverUrls.map((url, index) => ({
          url,
          description: serverDescriptions?.[index]
        }))
      ],
      paths: versionedPaths[OPENAPI_DEFAULT_VERSION]
    },
    ...Object.fromEntries(
      Object.entries(versionedPaths).map(([version, paths]) => [
        version,
        {
          openapi: '3.1.0',
          info: {
            title: title || process.env.API_TITLE || 'API Definition',
            version,
            description,
            contact
          },
          components: {
            securitySchemes: versionedSecuritySchemes[version]
          },
          tags: versionedTags[version],
          servers: [
            ...serverUrls.map((url, index) => ({
              url,
              description: serverDescriptions?.[index]
            }))
          ],
          paths
        }
      ])
    )
  };
}

/**
 * Resolves the content object for a given schema.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @param {SV} schemaValidator - The schema validator.
 * @param {IdiomaticSchema<SV>} body - The schema body.
 * @returns {ContentObject} - The resolved content object.
 */
function contentResolver<SV extends AnySchemaValidator>(
  schemaValidator: SV,
  body: IdiomaticSchema<SV>,
  contentType?: string
): ContentObject {
  const bodySpec = (schemaValidator as SchemaValidator).openapi(body);
  return contentType != null
    ? {
        [contentType]: {
          schema: bodySpec
        }
      }
    : body === schemaValidator.string
      ? {
          'text/plain': {
            schema: bodySpec
          }
        }
      : {
          'application/json': {
            schema: bodySpec
          }
        };
}

function generateOperationObject<SV extends AnySchemaValidator>(
  schemaValidator: SV,
  path: string,
  method: Method,
  controllerName: string,
  sdkPaths: Record<string, string>,
  securitySchemes: Record<string, SecuritySchemeObject>,
  name: string,
  summary: string,
  responses: Record<number, ResponseBody<SV>>,
  params?: ParamsObject<SV>,
  responseHeaders?: HeadersObject<SV>,
  requestHeaders?: HeadersObject<SV>,
  query?: QueryObject<SV>,
  body?: Body<SV>,
  auth?: SchemaAuthMethods<
    SV,
    ParamsObject<SV>,
    Body<SV>,
    QueryObject<SV>,
    HeadersObject<SV>,
    VersionSchema<SV, Method>,
    unknown
  >
) {
  const typedSchemaValidator = schemaValidator as SchemaValidator;

  const coercedResponses: ResponsesObject = {};

  const discriminatedResponseBodiesResult = discriminateResponseBodies(
    schemaValidator,
    responses
  );

  for (const key in discriminatedResponseBodiesResult) {
    coercedResponses[key] = {
      description: HTTPStatuses[key],
      content: contentResolver(
        schemaValidator,
        discriminatedResponseBodiesResult[key].schema,
        discriminatedResponseBodiesResult[key].contentType
      ),
      headers: responseHeaders
        ? Object.fromEntries(
            Object.entries(responseHeaders).map(([key, value]) => [
              key,
              {
                schema: typedSchemaValidator.openapi(value)
              }
            ])
          )
        : undefined
    } satisfies ResponseObject;
  }

  const commonErrors = [400, 404, 500];
  for (const error of commonErrors) {
    if (!(error in responses)) {
      coercedResponses[error] = {
        description: HTTPStatuses[error],
        content: contentResolver(schemaValidator, schemaValidator.string)
      };
    }
  }

  const operationObject: OperationObject = {
    tags: [controllerName],
    summary: `${name}: ${summary}`,
    parameters: [],
    responses: coercedResponses,
    operationId: sdkPaths[[method, path].join('.')]
  };

  if (params) {
    for (const key in params) {
      operationObject.parameters?.push({
        name: key,
        in: 'path',
        schema: typedSchemaValidator.openapi(params[key])
      });
    }
  }

  const discriminatedBodyResult = body
    ? discriminateBody(schemaValidator, body)
    : null;

  if (discriminatedBodyResult) {
    operationObject.requestBody = {
      required: true,
      content: contentResolver(
        schemaValidator,
        discriminatedBodyResult.schema,
        discriminatedBodyResult.contentType
      )
    };
  }

  if (requestHeaders) {
    for (const key in requestHeaders) {
      operationObject.parameters?.push({
        name: key,
        in: 'header',
        schema: typedSchemaValidator.openapi(requestHeaders[key])
      });
    }
  }

  if (query) {
    for (const key in query) {
      operationObject.parameters?.push({
        name: key,
        in: 'query',
        schema: typedSchemaValidator.openapi(query[key])
      });
    }
  }

  if (auth) {
    coercedResponses[401] = {
      description: HTTPStatuses[401],
      content: contentResolver(schemaValidator, schemaValidator.string)
    };

    coercedResponses[403] = {
      description: HTTPStatuses[403],
      content: contentResolver(schemaValidator, schemaValidator.string)
    };

    if ('basic' in auth) {
      operationObject.security = [
        {
          basic: Array.from(
            'allowedPermissions' in auth &&
              auth.allowedPermissions instanceof Set
              ? auth.allowedPermissions.values()
              : []
          )
        }
      ];

      securitySchemes['basic'] = {
        type: 'http',
        scheme: 'basic'
      };
    } else if (auth) {
      operationObject.security = [
        {
          [auth.headerName !== 'Authorization' ? 'bearer' : 'apiKey']:
            Array.from(
              'allowedPermissions' in auth &&
                auth.allowedPermissions instanceof Set
                ? auth.allowedPermissions.values()
                : []
            )
        }
      ];

      if (auth.headerName && auth.headerName !== 'Authorization') {
        securitySchemes[auth.headerName] = {
          type: 'apiKey',
          in: 'header',
          name: auth.headerName
        };
      } else {
        securitySchemes['Authorization'] = {
          type: 'http',
          scheme: 'bearer',
          bearerFormat: 'JWT'
        };
      }
    }
  }

  return operationObject;
}

/**
 * Generates a Swagger document from given routers.
 *
 * @template SV - A type that extends AnySchemaValidator.
 * @param {SV} schemaValidator - The schema validator.
 * @param {string | number} port - The port on which the server is running.
 * @param {ForklaunchRouter<SV>[]} routers - The routers to include in the Swagger document.
 * @returns {OpenAPIObject} - The generated Swagger document.
 */
export function generateOpenApiSpecs<SV extends AnySchemaValidator>(
  schemaValidator: SV,
  serverUrls: string[],
  serverDescriptions: string[],
  application: ForklaunchRouter<SV>,
  appOptions?: ExpressLikeApplicationOptions<SV, SessionObject<SV>>['openapi']
): Record<string | symbol, OpenAPIObject> {
  const versionedPaths: Record<string | symbol, PathObject> = {
    [OPENAPI_DEFAULT_VERSION]: {}
  };
  const versionedTags: Record<string | symbol, TagObject[]> = {
    [OPENAPI_DEFAULT_VERSION]: []
  };
  const versionedSecuritySchemes: Record<
    string | symbol,
    Record<string, SecuritySchemeObject>
  > = {
    [OPENAPI_DEFAULT_VERSION]: {}
  };

  [
    {
      fullPath: application.basePath === '/' ? '' : application.basePath,
      router: application
    },
    ...unpackRouters<SV>(application.routers, [
      application.basePath === '/' ? '' : application.basePath
    ])
  ].forEach(({ fullPath, router }) => {
    const controllerName = transformBasePath(fullPath);
    router.routes.forEach((route) => {
      const openApiPath = openApiCompliantPath(
        `${fullPath}${route.path === '/' ? '' : route.path}`
      );

      const { name, summary, params, versions, auth, options } =
        route.contractDetails;

      if (
        !(
          options?.openapi ??
          router.routerOptions?.openapi ??
          appOptions !== false
        )
      ) {
        return;
      }

      if (versions) {
        for (const version of Object.keys(versions)) {
          if (!versionedPaths[version]) {
            versionedPaths[version] = {};
          }
          if (!versionedPaths[version][openApiPath]) {
            versionedPaths[version][openApiPath] = {};
          }
          if (!versionedTags[version]) {
            versionedTags[version] = [];
          }
          if (
            !versionedTags[version].find((tag) => tag.name === controllerName)
          ) {
            versionedTags[version].push({
              name: controllerName,
              description: `${toUpperCase(controllerName)} Operations`
            });
          }
          if (!versionedSecuritySchemes[version]) {
            versionedSecuritySchemes[version] = {};
          }

          const { query, requestHeaders, body, responses, responseHeaders } =
            versions[version];

          const operationObject = generateOperationObject<SV>(
            schemaValidator,
            route.path,
            route.method,
            controllerName,
            router.sdkPaths,
            versionedSecuritySchemes[version],
            name,
            summary,
            responses,
            params,
            responseHeaders,
            requestHeaders,
            query,
            body,
            auth
          );

          if (route.method !== 'middleware') {
            versionedPaths[version][openApiPath][route.method] =
              operationObject;
          }
        }
      } else {
        if (!versionedPaths[OPENAPI_DEFAULT_VERSION]) {
          versionedPaths[OPENAPI_DEFAULT_VERSION] = {};
        }
        if (!versionedPaths[OPENAPI_DEFAULT_VERSION][openApiPath]) {
          versionedPaths[OPENAPI_DEFAULT_VERSION][openApiPath] = {};
        }
        if (!versionedTags[OPENAPI_DEFAULT_VERSION]) {
          versionedTags[OPENAPI_DEFAULT_VERSION] = [];
        }
        if (
          !versionedTags[OPENAPI_DEFAULT_VERSION].find(
            (tag) => tag.name === controllerName
          )
        ) {
          versionedTags[OPENAPI_DEFAULT_VERSION].push({
            name: controllerName,
            description: `${toUpperCase(controllerName)} Operations`
          });
        }
        if (!versionedSecuritySchemes[OPENAPI_DEFAULT_VERSION]) {
          versionedSecuritySchemes[OPENAPI_DEFAULT_VERSION] = {};
        }

        const { query, requestHeaders, body, responses, responseHeaders } =
          route.contractDetails;

        const operationObject = generateOperationObject<SV>(
          schemaValidator,
          route.path,
          route.method,
          controllerName,
          router.sdkPaths,
          versionedSecuritySchemes[OPENAPI_DEFAULT_VERSION],
          name,
          summary,
          responses,
          params,
          responseHeaders,
          requestHeaders,
          query,
          body,
          auth
        );

        if (route.method !== 'middleware') {
          versionedPaths[OPENAPI_DEFAULT_VERSION][openApiPath][route.method] =
            operationObject;
        }
      }
    });
  });

  return generateOpenApiDocument(
    serverUrls,
    serverDescriptions,
    versionedTags,
    versionedPaths,
    versionedSecuritySchemes,
    appOptions
  );
}
