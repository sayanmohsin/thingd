process.on('exit', (code) => console.log('Exiting with code:', code));
process.on('uncaughtException', (err) => console.log('uncaught:', err));
process.on('unhandledRejection', (err) => console.log('unhandled:', err));

console.log("Start");
const NATIVE = require("./packages/thingd-native/dist/thingd_native.node");
console.log("Required");

const db = NATIVE.NativeThingStore.open("/Users/sayanmohsin/Downloads/data.db");
console.log("Opened db");

const p = db.countObjectsJson();
console.log("Promise:", p);
p.then(res => console.log("Result:", res)).catch(err => console.log("Err:", err));
