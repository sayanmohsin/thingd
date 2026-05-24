const NATIVE = require("./packages/thingd-native/dist/thingd_native.node");
const db = NATIVE.NativeThingStore.open("/Users/sayanmohsin/Downloads/data.db");

async function run() {
  for (let i = 0; i < 10000; i++) {
    db.putObjectJson("test", "id" + i, '{"foo":"bar"}');
  }
  console.log("Done");
}
run();
