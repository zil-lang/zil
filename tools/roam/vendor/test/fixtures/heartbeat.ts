import{writeFileSync,appendFileSync,existsSync}from'node:fs';import{join}from'node:path';
const root=process.argv[2];writeFileSync(join(root,'pid'),String(process.pid));writeFileSync(join(root,'heartbeat'),'started\n');
setInterval(()=>{if(existsSync(join(root,'stop')))process.exit(0);appendFileSync(join(root,'heartbeat'),'beat\n');},20);
