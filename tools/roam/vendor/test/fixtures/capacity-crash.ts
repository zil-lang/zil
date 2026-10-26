import {FileStore} from '../../store';
const store=new FileStore(process.argv[2],1000000);
store.bytes=()=>{ process.exit(23); };
store.publish({format:'roam/1',contract:'capacity/1',steps:[]},{status:'error',steps:0,issues:[]});
