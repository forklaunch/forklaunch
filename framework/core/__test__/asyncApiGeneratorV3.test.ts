import type {
  ChannelObject,
  OperationObject
} from '@asyncapi/parser/esm/spec-types/v3';
import {
  mockSchemaValidator,
  optional,
  string
} from '@forklaunch/validator/tests/mockSchemaValidator';
import { describe, expect, it } from 'vitest';
import { generateAsyncApi } from '../src/ws/asyncApiV3Generator/asyncApiV3Generator';
import type { EventSchema } from '../src/ws/types/eventSchema.types';

describe('asyncApiGeneratorV3 tests', () => {
  const testMessageSchema = {
    message: string,
    timestamp: optional(string)
  };

  const testErrorSchema = {
    error: string,
    code: string
  };

  it.each([
    'room/~feed',
    'publish/~',
    'hello 水 🦀',
    'percent%2F',
    'space name',
    "quote!'()*",
    '__proto__',
    'constructor',
    'toString',
    'line\nbreak'
  ])('resolves escaped channel/message references for %j', (name) => {
    const result = generateAsyncApi<typeof mockSchemaValidator>({
      clientMessages: {
        request: { shape: testMessageSchema, channel: name, operation: name }
      },
      serverMessages: {}
    });
    const operation = Object.values(result.operations!)[0] as OperationObject;
    const resolve = (reference: string) =>
      decodeURIComponent(reference.slice(2))
        .split('/')
        .reduce<unknown>(
          (value, key) =>
            (value as Record<string, unknown>)[
              key.replace(/~1/g, '/').replace(/~0/g, '~')
            ],
          result
        );
    expect(Object.hasOwn(result.channels!, name)).toBe(true);
    expect(resolve((operation.channel as { $ref: string }).$ref)).toBe(
      result.channels![name]
    );
    const message = operation.messages![0];
    expect('$ref' in message).toBe(true);
    expect(resolve((message as { $ref: string }).$ref)).toEqual({
      name,
      payload: testMessageSchema
    });
    expect(Object.hasOwn(Object.prototype, 'messages')).toBe(false);
  });

  it.each(['#fragment', 'path?query', ''])(
    'rejects invalid channel addresses %j',
    (channel) => {
      expect(() =>
        generateAsyncApi<typeof mockSchemaValidator>({
          clientMessages: {
            request: { shape: testMessageSchema, channels: [channel] }
          },
          serverMessages: {}
        })
      ).toThrow('channel addresses');
    }
  );

  it('escapes fragment characters in message references', () => {
    const result = generateAsyncApi<typeof mockSchemaValidator>({
      clientMessages: {
        request: { shape: testMessageSchema, operation: '#fragment' }
      },
      serverMessages: {}
    });
    const operation = Object.values(result.operations!)[0] as OperationObject;
    expect((operation.messages![0] as { $ref: string }).$ref).toBe(
      '#/channels/request/messages/%23fragment'
    );
  });

  it('bounds recursive plain schema traversal', () => {
    let shape: unknown = {};
    for (let index = 0; index < 66; index++)
      shape = { properties: { next: shape } };
    expect(() =>
      generateAsyncApi<typeof mockSchemaValidator>({
        clientMessages: {
          request: { shape: shape as typeof testMessageSchema }
        },
        serverMessages: {}
      })
    ).toThrow('schema nesting exceeds');
  });

  it('should generate basic AsyncAPI document with client and server messages', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema
        }
      },
      serverMessages: {
        notification: {
          shape: testMessageSchema
        }
      }
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.asyncapi).toBe('3.0.0');
    expect(result.channels).toBeDefined();
    expect(result.channels?.userAction).toBeDefined();
    expect(result.channels?.notification).toBeDefined();
    expect(result.operations).toBeDefined();
  });

  it('should use explicit channel name when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema,
          channel: 'custom-channel'
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels?.['custom-channel']).toBeDefined();
    expect(result.channels?.userAction).toBeUndefined();
  });

  it('should create messages in all channels when channels array is provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema,
          channels: ['first-channel', 'second-channel']
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels?.['first-channel']).toBeDefined();
    expect(result.channels?.['second-channel']).toBeDefined();

    // Message should exist in both channels
    const firstChannel = result.channels?.['first-channel'] as ChannelObject;
    const secondChannel = result.channels?.['second-channel'] as ChannelObject;
    expect(firstChannel?.messages?.['userAction']).toBeDefined();
    expect(secondChannel?.messages?.['userAction']).toBeDefined();

    // Operations should be created for both channels
    expect(
      result.operations?.['receive-first-channel-userAction']
    ).toBeDefined();
    expect(
      result.operations?.['receive-second-channel-userAction']
    ).toBeDefined();
  });

  it('should use message key as channel when no channel specified', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        defaultChannelMessage: {
          shape: testMessageSchema
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels?.['defaultChannelMessage']).toBeDefined();
  });

  it('should use explicit operation name when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema,
          operation: 'custom-operation'
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const channel = result.channels?.['userAction'] as ChannelObject;
    expect(channel).toBeDefined();
    expect(channel?.messages?.['custom-operation']).toBeDefined();
    expect(channel?.messages?.['userAction']).toBeUndefined();
  });

  it('should use first operation from operations array when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema,
          operations: ['first-op', 'second-op']
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const channel = result.channels?.['userAction'] as ChannelObject;
    expect(channel?.messages?.['first-op']).toBeDefined();
    expect(channel?.messages?.['second-op']).toBeUndefined();
  });

  it('should use message key as operation when no operation specified', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        defaultOperationMessage: {
          shape: testMessageSchema
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const channel = result.channels?.[
      'defaultOperationMessage'
    ] as ChannelObject;
    expect(channel?.messages?.['defaultOperationMessage']).toBeDefined();
  });

  it('should create receive operation for clientMessages', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const operationKey = 'receive-userAction-userAction';
    const operation = result.operations?.[operationKey] as OperationObject;
    expect(operation).toBeDefined();
    expect(operation?.action).toBe('receive');
  });

  it('should create send operation for serverMessages', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {
        notification: {
          shape: testMessageSchema
        }
      }
    };

    const result = generateAsyncApi(eventSchemas);

    const operationKey = 'send-notification-notification';
    const operation = result.operations?.[operationKey] as OperationObject;
    expect(operation).toBeDefined();
    expect(operation?.action).toBe('send');
  });

  it('should include errors in channels', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {},
      errors: {
        validationError: {
          shape: testErrorSchema
        }
      }
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels?.['validationError']).toBeDefined();
    const channel = result.channels?.['validationError'] as ChannelObject;
    expect(channel?.messages?.['validationError']).toBeDefined();
  });

  it('should handle multiple messages in same channel', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        message1: {
          shape: testMessageSchema,
          channel: 'shared-channel'
        },
        message2: {
          shape: testMessageSchema,
          channel: 'shared-channel'
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const channel = result.channels?.['shared-channel'] as ChannelObject;
    expect(channel).toBeDefined();
    expect(channel?.messages?.['message1']).toBeDefined();
    expect(channel?.messages?.['message2']).toBeDefined();
  });

  it('should use custom title, version, and description when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas, {
      title: 'Custom API',
      version: '2.0.0',
      description: 'Custom description'
    });

    expect(result.info?.title).toBe('Custom API');
    expect(result.info?.version).toBe('2.0.0');
    expect(result.info?.description).toBe('Custom description');
  });

  it('should use default info when not provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.info?.title).toBe('Forklaunch WebSocket API');
    expect(result.info?.version).toBe('1.0.0');
  });

  it('should include id when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas, {
      id: 'custom-api-id'
    });

    expect(result.id).toBe('custom-api-id');
  });

  it('should not include id when not provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.id).toBeUndefined();
  });

  it('should include defaultContentType when provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas, {
      defaultContentType: 'application/json'
    });

    expect(result.defaultContentType).toBe('application/json');
  });

  it('should not include defaultContentType when not provided', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.defaultContentType).toBeUndefined();
  });

  it('should not include channels when empty', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels).toBeUndefined();
  });

  it('should not include operations when empty', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.operations).toBeUndefined();
  });

  it('should create correct operation references', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userAction: {
          shape: testMessageSchema,
          channel: 'user-channel',
          operation: 'user-op'
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    const operation = result.operations?.[
      'receive-user-channel-user-op'
    ] as OperationObject;
    expect(operation).toBeDefined();
    expect(operation?.channel).toEqual({
      $ref: '#/channels/user-channel'
    });
    expect(operation?.messages).toEqual([
      {
        $ref: '#/channels/user-channel/messages/user-op'
      }
    ]);
  });

  it('should handle complex scenario with all message types', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        clientMsg1: {
          shape: testMessageSchema
        },
        clientMsg2: {
          shape: testMessageSchema,
          channel: 'shared',
          operation: 'client-op'
        }
      },
      serverMessages: {
        serverMsg1: {
          shape: testMessageSchema
        },
        serverMsg2: {
          shape: testMessageSchema,
          channel: 'shared',
          operation: 'server-op'
        }
      },
      errors: {
        error1: {
          shape: testErrorSchema
        }
      }
    };

    const result = generateAsyncApi(eventSchemas, {
      title: 'Test API',
      version: '1.0.0',
      id: 'test-api',
      defaultContentType: 'application/json'
    });

    // Check channels
    expect(result.channels?.['clientMsg1']).toBeDefined();
    expect(result.channels?.['serverMsg1']).toBeDefined();
    expect(result.channels?.['shared']).toBeDefined();
    expect(result.channels?.['error1']).toBeDefined();

    // Check shared channel has both messages
    const sharedChannel = result.channels?.['shared'] as ChannelObject;
    expect(sharedChannel?.messages?.['client-op']).toBeDefined();
    expect(sharedChannel?.messages?.['server-op']).toBeDefined();

    // Check operations
    expect(result.operations?.['receive-clientMsg1-clientMsg1']).toBeDefined();
    expect(result.operations?.['receive-shared-client-op']).toBeDefined();
    expect(result.operations?.['send-serverMsg1-serverMsg1']).toBeDefined();
    expect(result.operations?.['send-shared-server-op']).toBeDefined();

    // Check options
    expect(result.info?.title).toBe('Test API');
    expect(result.id).toBe('test-api');
    expect(result.defaultContentType).toBe('application/json');
  });

  it('should skip non-valid entries in collections', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        validMessage: {
          shape: testMessageSchema
        },
        nullMessage: null as unknown as EventSchema<
          typeof mockSchemaValidator
        >['clientMessages'][string],
        stringMessage: 'not-an-object' as unknown as EventSchema<
          typeof mockSchemaValidator
        >['clientMessages'][string],
        arrayMessage: [] as unknown as EventSchema<
          typeof mockSchemaValidator
        >['clientMessages'][string]
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    expect(result.channels?.['validMessage']).toBeDefined();
    expect(result.channels?.['nullMessage']).toBeUndefined();
    expect(result.channels?.['stringMessage']).toBeUndefined();
    expect(result.channels?.['arrayMessage']).toBeUndefined();
  });

  it('should handle ping and pong messages', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      ping: {
        shape: testMessageSchema
      },
      pong: {
        shape: testMessageSchema
      },
      clientMessages: {},
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    // Note: ping and pong are not currently processed by the generator
    // This test documents current behavior
    expect(result.channels).toBeUndefined();
  });

  it('should create operations for all channels when multiple channels specified', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        multiChannelMessage: {
          shape: testMessageSchema,
          channels: ['channel-a', 'channel-b', 'channel-c'],
          operation: 'multi-op'
        }
      },
      serverMessages: {}
    };

    const result = generateAsyncApi(eventSchemas);

    // All channels should exist
    expect(result.channels?.['channel-a']).toBeDefined();
    expect(result.channels?.['channel-b']).toBeDefined();
    expect(result.channels?.['channel-c']).toBeDefined();

    // Message should be in all channels
    const channelA = result.channels?.['channel-a'] as ChannelObject;
    const channelB = result.channels?.['channel-b'] as ChannelObject;
    const channelC = result.channels?.['channel-c'] as ChannelObject;
    expect(channelA?.messages?.['multi-op']).toBeDefined();
    expect(channelB?.messages?.['multi-op']).toBeDefined();
    expect(channelC?.messages?.['multi-op']).toBeDefined();

    // Operations should be created for all channels
    expect(result.operations?.['receive-channel-a-multi-op']).toBeDefined();
    expect(result.operations?.['receive-channel-b-multi-op']).toBeDefined();
    expect(result.operations?.['receive-channel-c-multi-op']).toBeDefined();
  });

  it('should use process.env.API_TITLE as fallback for title', () => {
    const originalTitle = process.env.API_TITLE;
    process.env.API_TITLE = 'Env API Title';

    try {
      const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
        clientMessages: {},
        serverMessages: {}
      };

      const result = generateAsyncApi(eventSchemas);

      expect(result.info?.title).toBe('Env API Title');
    } finally {
      if (originalTitle) {
        process.env.API_TITLE = originalTitle;
      } else {
        delete process.env.API_TITLE;
      }
    }
  });

  it('should use default title when no title provided and no env var', () => {
    const originalTitle = process.env.API_TITLE;
    delete process.env.API_TITLE;

    try {
      const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
        clientMessages: {},
        serverMessages: {}
      };

      const result = generateAsyncApi(eventSchemas);

      expect(result.info?.title).toBe('Forklaunch WebSocket API');
    } finally {
      if (originalTitle) {
        process.env.API_TITLE = originalTitle;
      }
    }
  });

  it('should generate exact AsyncAPI document structure', () => {
    const eventSchemas: EventSchema<typeof mockSchemaValidator> = {
      clientMessages: {
        userLogin: {
          shape: testMessageSchema,
          channel: 'user-events',
          operation: 'login'
        },
        userLogout: {
          shape: testMessageSchema,
          channel: 'user-events',
          operation: 'logout'
        },
        systemMessage: {
          shape: testMessageSchema
        }
      },
      serverMessages: {
        notification: {
          shape: testMessageSchema,
          channel: 'notifications',
          operation: 'send-notification'
        },
        broadcast: {
          shape: testMessageSchema
        }
      },
      errors: {
        validationError: {
          shape: testErrorSchema,
          channel: 'errors',
          operation: 'validation-failed'
        }
      }
    };

    const result = generateAsyncApi(eventSchemas, {
      title: 'Test WebSocket API',
      version: '2.0.0',
      description: 'Test API Description',
      id: 'test-ws-api',
      defaultContentType: 'application/json'
    });

    // Verify exact document structure
    expect(result).toEqual({
      asyncapi: '3.0.0',
      info: {
        title: 'Test WebSocket API',
        version: '2.0.0',
        description: 'Test API Description'
      },
      id: 'test-ws-api',
      defaultContentType: 'application/json',
      channels: {
        'user-events': {
          address: 'user-events',
          messages: {
            login: {
              name: 'login',
              payload: testMessageSchema
            },
            logout: {
              name: 'logout',
              payload: testMessageSchema
            }
          }
        },
        systemMessage: {
          address: 'systemMessage',
          messages: {
            systemMessage: {
              name: 'systemMessage',
              payload: testMessageSchema
            }
          }
        },
        notifications: {
          address: 'notifications',
          messages: {
            'send-notification': {
              name: 'send-notification',
              payload: testMessageSchema
            }
          }
        },
        broadcast: {
          address: 'broadcast',
          messages: {
            broadcast: {
              name: 'broadcast',
              payload: testMessageSchema
            }
          }
        },
        errors: {
          address: 'errors',
          messages: {
            'validation-failed': {
              name: 'validation-failed',
              payload: testErrorSchema
            }
          }
        }
      },
      operations: {
        'receive-user-events-login': {
          action: 'receive',
          channel: {
            $ref: '#/channels/user-events'
          },
          messages: [
            {
              $ref: '#/channels/user-events/messages/login'
            }
          ]
        },
        'receive-user-events-logout': {
          action: 'receive',
          channel: {
            $ref: '#/channels/user-events'
          },
          messages: [
            {
              $ref: '#/channels/user-events/messages/logout'
            }
          ]
        },
        'receive-systemMessage-systemMessage': {
          action: 'receive',
          channel: {
            $ref: '#/channels/systemMessage'
          },
          messages: [
            {
              $ref: '#/channels/systemMessage/messages/systemMessage'
            }
          ]
        },
        'send-notifications-send-notification': {
          action: 'send',
          channel: {
            $ref: '#/channels/notifications'
          },
          messages: [
            {
              $ref: '#/channels/notifications/messages/send-notification'
            }
          ]
        },
        'send-broadcast-broadcast': {
          action: 'send',
          channel: {
            $ref: '#/channels/broadcast'
          },
          messages: [
            {
              $ref: '#/channels/broadcast/messages/broadcast'
            }
          ]
        }
      }
    });
  });
});
