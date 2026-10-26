import {FileStore} from '../../store';
const store = new FileStore(process.argv[2]);
console.log(store.claim('parallel', process.argv[3], 10000)?.token ?? 'none');
