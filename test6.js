console.log("Start");
const NATIVE = require("./packages/thingd-native/dist/thingd_native.node");
const db = NATIVE.NativeThingStore.open("/Users/sayanmohsin/Downloads/data.db");
console.log("Opened");
try {
  db.putObjectJson("test", "id1", '{"foo":"bar"}');
  console.log("Inserted");
} catch (e) {
  console.error("Error:", e);
}
