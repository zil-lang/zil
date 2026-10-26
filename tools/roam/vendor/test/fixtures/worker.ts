import { createInterface } from 'node:readline';
import {serveWorker} from '../../worker';
const mode = process.argv[2];
if (['badissues','postterminal','extrabefore','hiddenfailure'].includes(mode)) {
    const lines=createInterface({input:process.stdin});process.stdout.write(JSON.stringify({type:'hello',format:'roam-worker/1',capabilities:[]})+'\n');
    lines.on('line',line=>{if(JSON.parse(line).type!=='run')return;
        if(mode==='extrabefore')process.stdout.write(JSON.stringify({type:'before',step:0,action:{op:'extra'}})+'\n');
        else{process.stdout.write(JSON.stringify({type:'result',result:{status:'pass',steps:0,issues:mode==='badissues'?[{}]:[],...(mode==='hiddenfailure'?{failure:{propertyId:'hidden',message:'must not pass'}}:{})}})+'\n');if(mode==='postterminal')process.stdout.write(JSON.stringify({type:'before',step:0,action:{op:'extra'}})+'\n');}
        lines.close();process.stdin.pause();
    });
}
else if (mode === 'reorder') {
    const lines = createInterface({input:process.stdin}); let trace:any;
    process.stdout.write(JSON.stringify({type:'hello',format:'roam-worker/1',capabilities:[]})+'\n');
    lines.on('line', line=>{ const message=JSON.parse(line);
        if(message.type==='run'){trace=message.case;const action=Object.fromEntries(Object.entries(trace.steps[0]).reverse());process.stdout.write(JSON.stringify({type:'before',step:0,action})+'\n');}
        else if(message.type==='ack'){ process.stdout.write(JSON.stringify({type:'after',step:0})+'\n');process.stdout.write(JSON.stringify({type:'result',result:{status:'pass',steps:1,issues:[]}})+'\n');lines.close();process.stdin.pause(); }
    });
}
else if (mode === 'nohello') setInterval(() => {}, 1000);
else if (mode === 'malformed') process.stdout.write('not json\n');
else if (mode === 'spawnerror') process.exit(12);
else await serveWorker(async (trace, before, after) => {
    for (let i = 0; i < trace.steps.length; i++) {
        await before(i);
        if (mode === 'crash') process.exit(17);
        if (mode === 'hang') await new Promise(() => {});
        after(i);
    }
    if (mode === 'hang') setInterval(() => {}, 1000);
    if (mode === 'underreport') return {status:'error',steps:0,issues:[]};
    return mode === 'fail' ? { status: 'fail', steps: trace.steps.length, issues: [], failure: { propertyId: 'value', message: 'wrong' } } : { status: 'pass', steps: trace.steps.length, issues: [] };
}, ['fixture']);
