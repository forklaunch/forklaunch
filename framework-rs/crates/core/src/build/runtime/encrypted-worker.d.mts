import type {ChannelKey, ConsumerOptions, ChannelContext} from './encrypted-channel.mjs';
export interface SealedWorkEvent {
  channel: 'work-v1';
  context: {tenantId: string; resourceId: string; operationId: string; revision: number};
  ciphertext: string;
}
export interface SecureWorkInput { resourceId: string; revision: number }
export type WorkPublisherOptions = {namespace:string; tenantId:string; previousKeys?:readonly ChannelKey[]; maxPayloadBytes?:number; publish(event:SealedWorkEvent):Promise<void>} & ({activeKey:ChannelKey} | {keyId:string;key:Uint8Array});
export function createEncryptedWorkPublisher(options:WorkPublisherOptions): {
  close():void;
  sealAndPublish(request:{channel:'work-v1';queue:string;operationId:string;input:SecureWorkInput}):Promise<void>;
};
export function createAuditedWorkConsumer<T>(options:ConsumerOptions, queue:string, validate:(value:unknown)=>value is T): {
  consume(envelope:unknown,expected:ChannelContext):Promise<T>;
  close():void;
};
/** Low-level compatibility API: no schema validation or access audit. */

export function createEncryptedWorkConsumer(options: {
  namespace: string; tenantId: string; keyId: string; key: Uint8Array;
}): {
  close(): void;
  open(envelope: unknown, trusted: {queue: string; operationId: string}): {
    context: SealedWorkEvent['context']; queue: string; input: unknown;
  };
};
