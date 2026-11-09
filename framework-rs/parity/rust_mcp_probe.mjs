import assert from 'node:assert/strict';
export async function mcpRpc(base,method,params={},headers={},endpoint='/mcp') {
  const response=await fetch(`${base}${endpoint}`,{method:'POST',headers:{'content-type':'application/json',accept:'application/json, text/event-stream','mcp-protocol-version':'2025-11-25',...headers},body:JSON.stringify({jsonrpc:'2.0',id:1,method,params}),signal:AbortSignal.timeout(10000)});
  assert.equal(response.status,200,await response.clone().text());
  const body=await response.json();assert.ok(!body.error,JSON.stringify(body));return body.result;
}
export async function probeMcp(base,requiredTools=[],endpoint='/mcp',httpBase=base) {
  const initialize=await mcpRpc(base,'initialize',{protocolVersion:'2025-11-25',capabilities:{},clientInfo:{name:'forklaunch-startup-fixture',version:'1.0.0'}},{},endpoint);
  assert.ok(initialize.capabilities.tools);assert.ok(initialize.serverInfo.name);
  const {tools}=await mcpRpc(base,'tools/list',{},{},endpoint);
  for(const name of requiredTools)assert.ok(tools.some(tool=>tool.name===name),`Missing MCP tool ${name}`);
  const health=tools.find(tool=>['getHealth','health'].includes(tool.name));assert.ok(health,'Service must expose MCP health');
  const result=await mcpRpc(base,'tools/call',{name:health.name,arguments:{}},{},endpoint);assert.equal(result.isError,false,JSON.stringify(result));
  const http=await fetch(`${httpBase}/health`,{signal:AbortSignal.timeout(10000)});assert.equal(http.status,200);assert.deepEqual(JSON.parse(result.content[0].text),await http.json());
  const origin=await fetch(`${base}${endpoint}`,{method:'POST',headers:{origin:'https://attacker.invalid',accept:'application/json, text/event-stream','content-type':'application/json'},body:JSON.stringify({jsonrpc:'2.0',id:1,method:'tools/list'})});assert.equal(origin.status,403);
  console.log(`PASS HTTP/MCP startup, initialization, discovery, health invocation and Origin rejection: ${initialize.serverInfo.name}`);
  return tools;
}
