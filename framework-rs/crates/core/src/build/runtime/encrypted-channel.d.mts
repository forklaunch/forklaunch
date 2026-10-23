export interface ChannelContext { readonly tenantId: string; readonly resourceId: string; readonly operationId: string; readonly revision: number }
export type ContextFields = {readonly [P in keyof ChannelContext]: string | readonly string[]};
export interface ChannelDescriptor {readonly purpose: string; readonly contextFields: ContextFields; readonly wire: {readonly kind: 'sealed-event'} | {readonly kind: 'flat'; readonly contextFields: {readonly [P in keyof ChannelContext]: string}; readonly ciphertextField: string}}
export interface ChannelKey {readonly id: string; readonly key: Uint8Array}
export interface ChannelOptions {readonly namespace: string; readonly tenantId: string; readonly activeKey: ChannelKey; readonly previousKeys?: readonly ChannelKey[]; readonly maxPayloadBytes?: number; readonly auditTimeoutMs?: number}
export interface AuditRecord {readonly purpose: string; readonly actor: string; readonly context: ChannelContext}
export interface ProducerOptions extends ChannelOptions {publish(envelope: unknown): Promise<void>}
export interface ConsumerOptions extends ChannelOptions {readonly actor: string; audit(record: AuditRecord): Promise<true>}
export function createEncryptedChannelProducer<T>(descriptor: ChannelDescriptor, options: ProducerOptions, validate: (value: unknown) => value is T): {publish(payload: T, expected: ChannelContext): Promise<void>; close(): void};
export function createEncryptedChannelConsumer<T>(descriptor: ChannelDescriptor, options: ConsumerOptions, validate: (value: unknown) => value is T): {consume(envelope: unknown, expected: ChannelContext): Promise<T>; close(): void};
