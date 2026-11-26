// Native v3 envelope protocol. Application DTOs are encrypted directly, without a work-v1 wrapper.
import {createCipheriv, createDecipheriv, hkdfSync, randomBytes} from 'node:crypto';
const unavailable = () => new Error('Encrypted channel unavailable');
const uuid = value => typeof value === 'string' && /^[0-9a-f]{8}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{4}-[0-9a-f]{12}$/.test(value) && value !== '00000000-0000-0000-0000-000000000000';
const contextKeys = ['tenantId','resourceId','operationId','revision'];
const object = value => value !== null && typeof value === 'object' && !Array.isArray(value);
const exact = (value, names) => object(value) && Object.keys(value).sort().join(',') === [...names].sort().join(',');
const validContext = value => exact(value, contextKeys) && uuid(value.tenantId) && uuid(value.resourceId) && uuid(value.operationId) && Number.isSafeInteger(value.revision) && value.revision > 0;
const same = (a,b) => contextKeys.every(key => a[key] === b[key]);
const path = field => typeof field === 'string' ? [field] : field;
const validPath = field => Array.isArray(path(field)) && path(field).length > 0 && path(field).length <= 8 && path(field).every(key => typeof key === 'string' && key.length > 0 && !['__proto__','constructor','prototype'].includes(key));
function context(value, fields) {
  return Object.fromEntries(contextKeys.map(key => [key,path(fields[key]).reduce((object,field) => object !== null && typeof object === 'object' && Object.hasOwn(object,field) ? object[field] : undefined,value)]));
}
function configure(descriptor, options, validate) {
  if (!object(descriptor) || !/^[A-Za-z0-9-]{1,64}$/.test(descriptor.purpose) || !exact(descriptor.contextFields,contextKeys) ||
      !contextKeys.every(key => validPath(descriptor.contextFields[key])) ||
      new Set(Object.values(descriptor.contextFields).map(field => JSON.stringify(path(field)))).size !== 4 || typeof validate !== 'function' ||
      !/^[A-Za-z0-9_-]{1,64}$/.test(options.namespace) || !uuid(options.tenantId)) throw unavailable();
  const wire = descriptor.wire;
  if (!object(wire) || !['sealed-event','flat'].includes(wire.kind) ||
      (wire.kind === 'flat' && (!exact(wire.contextFields,contextKeys) || !contextKeys.every(key => typeof wire.contextFields[key] === 'string' && validPath(wire.contextFields[key])) || typeof wire.ciphertextField !== 'string' || !validPath(wire.ciphertextField) ||
       new Set([...Object.values(wire.contextFields),wire.ciphertextField]).size !== 5))) throw unavailable();
  const limit = options.maxPayloadBytes ?? 1048576;
  const timeout = options.auditTimeoutMs ?? 5000;
  if (!Number.isSafeInteger(limit) || limit < 1 || limit > 1048576 || !Number.isSafeInteger(timeout) || timeout < 1 || timeout > 30000) throw unavailable();
  const keys = new Map();
  try {
    for (const {id,key} of [options.activeKey,...(options.previousKeys ?? [])]) {
      if (!/^[A-Za-z0-9_-]{1,32}$/.test(id) || keys.has(id) || !(key instanceof Uint8Array) || key.length !== 32 || keys.size >= 5) throw unavailable();
      keys.set(id,Buffer.from(hkdfSync('sha256',key,Buffer.from('forklaunch-envelope'),Buffer.from(`${options.namespace}:v3:${options.tenantId}`),32)));
    }
    const config = {purpose:descriptor.purpose,fields:structuredClone(descriptor.contextFields),wire:structuredClone(wire),namespace:options.namespace,tenant:options.tenantId,active:options.activeKey.id,keys,limit,timeout,validate,closed:false};
    return config;
  } catch {
    for (const key of keys.values()) key.fill(0);
    keys.clear();
    throw unavailable();
  }
}
function trusted(config, expected) {
  if (config.closed || !validContext(expected) || expected.tenantId !== config.tenant) throw unavailable();
  return {...expected};
}
function wrap(config, expected, ciphertext) {
  if (config.wire.kind === 'sealed-event') return {channel:config.purpose,context:expected,ciphertext};
  return {...Object.fromEntries(contextKeys.map(key => [config.wire.contextFields[key],expected[key]])),[config.wire.ciphertextField]:ciphertext};
}
function unwrap(config, envelope) {
  if (config.wire.kind === 'sealed-event') {
    if (!exact(envelope,['channel','context','ciphertext']) || envelope.channel !== config.purpose || !validContext(envelope.context)) throw unavailable();
    return {context:envelope.context,ciphertext:envelope.ciphertext};
  }
  if (!exact(envelope,[...Object.values(config.wire.contextFields),config.wire.ciphertextField])) throw unavailable();
  return {context:context(envelope,config.wire.contextFields),ciphertext:envelope[config.wire.ciphertextField]};
}
function close(config) { config.closed=true; for (const key of config.keys.values()) key.fill(0); config.keys.clear(); }
function aad(config,id,expected) { return Buffer.from(`${config.namespace}:v3:${id}:${expected.tenantId}:${expected.operationId}:${config.purpose}`); }
export function createEncryptedChannelProducer(descriptor, options, validate) {
  const config=configure(descriptor,options,validate);
  if(typeof options.publish !== 'function') {close(config);throw unavailable();}
  const publish=options.publish;
  return {
    close() {close(config);},
    async publish(payload, expected) {
      let bytes;
      let envelope;
      try {
        expected=trusted(config,expected);
        // Validate exactly the JSON bytes that will be encrypted. toJSON/getters
        // can change an otherwise valid in-memory DTO during serialization.
        bytes=Buffer.from(JSON.stringify(payload));
        if(bytes.length > config.limit) throw unavailable();
        const snapshot=JSON.parse(bytes.toString('utf8'));
        if(!config.validate(snapshot) || !same(context(snapshot,config.fields),expected)) throw unavailable();
        const nonce=randomBytes(12);
        const cipher=createCipheriv('aes-256-gcm',config.keys.get(config.active),nonce);
        cipher.setAAD(aad(config,config.active,expected));
        const encrypted=Buffer.concat([cipher.update(bytes),cipher.final(),cipher.getAuthTag()]);
        envelope=wrap(config,expected,`v3.${config.active}.${nonce.toString('base64url')}.${encrypted.toString('base64url')}`);
      } catch {throw unavailable();} finally {bytes?.fill(0);}
      await publish(envelope);
    }
  };
}
export function createEncryptedChannelConsumer(descriptor, options, validate) {
  const config=configure(descriptor,options,validate);
  if(typeof options.audit !== 'function' || typeof options.actor !== 'string' || !options.actor.length || Buffer.byteLength(options.actor,'utf8') > 256 || /[\u0000-\u001f\u007f-\u009f]/u.test(options.actor)) {close(config);throw unavailable();}
  const audit=options.audit,actor=options.actor;
  return {
    close() {close(config);},
    async consume(envelope, expected) {
      let bytes, partial, timer;
      try {
        expected=trusted(config,expected);
        const opened=unwrap(config,envelope);
        if(!same(opened.context,expected) || typeof opened.ciphertext !== 'string' || opened.ciphertext.length > 1500000) throw unavailable();
        const parts=opened.ciphertext.split('.');
        if(parts.length !== 4 || parts[0] !== 'v3' || !config.keys.has(parts[1]) || !/^[A-Za-z0-9_-]{16}$/.test(parts[2]) || !/^[A-Za-z0-9_-]+$/.test(parts[3])) throw unavailable();
        const encrypted=Buffer.from(parts[3],'base64url');
        if(encrypted.length < 16 || encrypted.length > config.limit+16 || encrypted.toString('base64url') !== parts[3]) throw unavailable();
        const decipher=createDecipheriv('aes-256-gcm',config.keys.get(parts[1]),Buffer.from(parts[2],'base64url'));
        decipher.setAAD(aad(config,parts[1],expected));decipher.setAuthTag(encrypted.subarray(-16));
        partial=decipher.update(encrypted.subarray(0,-16));bytes=Buffer.concat([partial,decipher.final()]);
        const payload=JSON.parse(new TextDecoder('utf-8',{fatal:true}).decode(bytes));
        if(!config.validate(payload) || !same(context(payload,config.fields),expected)) throw unavailable();
        const receipt=await Promise.race([
          Promise.resolve().then(()=>audit(Object.freeze({purpose:config.purpose,actor,context:Object.freeze({...expected})}))),
          new Promise((_,reject)=>{timer=setTimeout(()=>reject(unavailable()),config.timeout);})
        ]);
        // The callback must acknowledge durable acceptance, rather than fire-and-forget.
        if(receipt !== true || config.closed) throw unavailable();
        return payload;
      } catch {throw unavailable();} finally {clearTimeout(timer);partial?.fill(0);bytes?.fill(0);}
    }
  };
}
