// Small RESP2 transport for actual-TypeScript interoperability fixtures.
import {createConnection} from 'node:net';

function parse(buffer,offset=0){
 if(offset>=buffer.length)return null;
 const end=buffer.indexOf('\r\n',offset);if(end<0)return null;
 const kind=String.fromCharCode(buffer[offset]),header=buffer.toString('utf8',offset+1,end);let next=end+2;
 if(kind==='+')return [header,next];
 if(kind==='-')throw new Error('Redis fixture command failed');
 if(kind===':')return [Number(header),next];
 const count=Number(header);
 if(kind==='$'){
  if(count===-1)return [null,next];
  if(buffer.length<next+count+2)return null;
  return [buffer.toString('utf8',next,next+count),next+count+2];
 }
 if(kind==='*'){
  if(count===-1)return [null,next];const values=[];
  for(let index=0;index<count;index++){const item=parse(buffer,next);if(!item)return null;values.push(item[0]);next=item[1];}
  return [values,next];
 }
 throw new Error('Unsupported Redis fixture response');
}

export function redisTransport(source){
 const url=new URL(source);if(url.protocol!=='redis:')throw new Error('Redis fixture transport requires redis://');
 async function perform(commands){
  const setup=[];
  if(url.password)setup.push(['AUTH',...(url.username?[decodeURIComponent(url.username)]:[]),decodeURIComponent(url.password)]);
  if(url.pathname&&url.pathname!=='/')setup.push(['SELECT',url.pathname.slice(1)]);
  const all=[...setup,...commands];
  return new Promise((resolve,reject)=>{
   let pending=Buffer.alloc(0);const replies=[];
   const socket=createConnection({host:url.hostname,port:Number(url.port||6379)});
   socket.setTimeout(10000,()=>socket.destroy(new Error('Redis fixture timed out')));
   socket.on('error',reject);
   socket.on('connect',()=>{
    const chunks=[];
    for(const command of all){chunks.push(Buffer.from(`*${command.length}\r\n`));for(const argument of command){const bytes=Buffer.from(String(argument));chunks.push(Buffer.from(`$${bytes.length}\r\n`),bytes,Buffer.from('\r\n'));}}
    socket.write(Buffer.concat(chunks));
   });
   socket.on('data',chunk=>{
    pending=Buffer.concat([pending,chunk]);
    try{for(;;){const reply=parse(pending);if(!reply)break;replies.push(reply[0]);pending=pending.subarray(reply[1]);if(replies.length===all.length){socket.end();resolve(replies.slice(setup.length));break;}}}
    catch(error){socket.destroy();reject(error);}
   });
   socket.on('end',()=>{if(replies.length!==all.length)reject(new Error('Redis fixture closed before its replies'));});
  });
 }
 const command=async(...args)=>(await perform([args]))[0];
 const transaction=async(commands)=>(await perform([['MULTI'],...commands,['EXEC']])).at(-1);
 return {command,transaction};
}
