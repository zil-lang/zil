import {FileStore} from '../../store';
const store=new FileStore(process.argv[2]);
(store as any).role=()=>process.exit(24);
store.claim('confirm','dead');
