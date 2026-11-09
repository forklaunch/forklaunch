// Work-v1 layout adapter over the shared native encrypted-channel boundary.
import {createDecipheriv, hkdfSync} from 'node:crypto';
import {createEncryptedChannelProducer,createEncryptedChannelConsumer} from './encrypted-channel.mjs';
const reject = () => new Error('Worker encrypted event unavailable');
const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) && value !== '00000000-0000-0000-0000-000000000000';
const exact=(value,names)=>value !== null && typeof value === 'object' && !Array.isArray(value) && Object.keys(value).sort().join(',') === [...names].sort().join(',');
const descriptor={purpose:'work-v1',contextFields:{tenantId:['context','tenantId'],resourceId:['context','resourceId'],operationId:['context','operationId'],revision:['context','revision']},wire:{kind:'sealed-event'}};
function validEvent(event) {
  return exact(event,['context','queue','input']) && exact(event.context,['tenantId','resourceId','operationId','revision']) &&
    typeof event.queue === 'string' && event.queue.length > 0 && event.queue.length <= 1024 &&
    event.input !== null && typeof event.input === 'object' && !Array.isArray(event.input) &&
    event.input.resourceId === event.context.resourceId && event.input.revision === event.context.revision;
}
export function createEncryptedWorkPublisher(options) {
  const tenantId=options.tenantId;
  const producer=createEncryptedChannelProducer(descriptor,{...options,activeKey:options.activeKey ?? {id:options.keyId,key:options.key}},validEvent);
  return {
    close(){producer.close();},
    async sealAndPublish({channel,queue,operationId,input}) {
      try {
        if(channel !== 'work-v1' || !input) throw reject();
        const context={tenantId,resourceId:input.resourceId,operationId,revision:input.revision};
        await producer.publish({context,queue,input},context);
      } catch {throw reject();}
    }
  };
}
/** Full DTO and queue validation precede acknowledged access audit and plaintext release. */
export function createAuditedWorkConsumer(options, queue, validate) {
  if(typeof queue !== 'string' || !queue.length || queue.length > 1024 || typeof validate !== 'function') throw reject();
  return createEncryptedChannelConsumer(descriptor,options,value => validEvent(value) && value.queue === queue && validate(value));
}

/** LOW-LEVEL LEGACY OPEN: authenticates only, with no access audit or DTO schema.
 * Do not use for application disclosure; generated WorkConsumer uses the audited path. */
export function createEncryptedWorkConsumer({namespace, tenantId, keyId, key}) {
  if (!/^[A-Za-z0-9_-]{1,64}$/.test(namespace) || !/^[A-Za-z0-9_-]{1,32}$/.test(keyId) ||
      !uuid(tenantId) || !(key instanceof Uint8Array) || key.length !== 32) throw reject();
  const derived = Buffer.from(hkdfSync('sha256', key, Buffer.from('forklaunch-envelope'), Buffer.from(`${namespace}:v3:${tenantId}`), 32));
  let closed = false;
  const keys = (value, expected) => value && typeof value === 'object' && Object.keys(value).sort().join(',') === expected;
  return {
    close() { closed = true; derived.fill(0); },
    open(envelope, {queue, operationId}) {
      let plaintext, partial;
      try {
        if (closed || !keys(envelope, 'channel,ciphertext,context') || envelope.channel !== 'work-v1' ||
            !keys(envelope.context, 'operationId,resourceId,revision,tenantId') || envelope.context.tenantId !== tenantId ||
            envelope.context.operationId !== operationId || !uuid(operationId) || !uuid(envelope.context.resourceId) ||
            !Number.isSafeInteger(envelope.context.revision) || envelope.context.revision < 1 ||
            typeof queue !== 'string' || !queue.length || typeof envelope.ciphertext !== 'string' || envelope.ciphertext.length > 1500000) throw reject();
        const parts = envelope.ciphertext.split('.');
        if(parts.length !== 4 || parts[0] !== 'v3' || parts[1] !== keyId ||
            !/^[A-Za-z0-9_-]{16}$/.test(parts[2]) || !/^[A-Za-z0-9_-]+$/.test(parts[3])) throw reject();
        const nonce = Buffer.from(parts[2], 'base64url');
        const encrypted = Buffer.from(parts[3], 'base64url');
        if(encrypted.length < 16 || encrypted.toString('base64url') !== parts[3]) throw reject();
        const cipher = createDecipheriv('aes-256-gcm', derived, nonce);
        cipher.setAAD(Buffer.from(`${namespace}:v3:${keyId}:${tenantId}:${operationId}:work-v1`));
        cipher.setAuthTag(encrypted.subarray(-16));
        partial = cipher.update(encrypted.subarray(0,-16));
        plaintext = Buffer.concat([partial, cipher.final()]);
        if(plaintext.length > 1048576) throw reject();
        const event = JSON.parse(plaintext.toString('utf8'));
        if(!keys(event, 'context,input,queue') || !keys(event.context, 'operationId,resourceId,revision,tenantId') || event.queue !== queue ||
            Object.entries(envelope.context).some(([key,value]) => event.context[key] !== value) ||
            !event.input || event.input.resourceId !== event.context.resourceId || event.input.revision !== event.context.revision) throw reject();
        return event;
      } catch { throw reject(); } finally { partial?.fill(0); plaintext?.fill(0); }
    }
  };
}
