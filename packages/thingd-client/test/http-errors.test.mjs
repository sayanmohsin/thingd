import { runHttpClientErrorContract } from "../../../scripts/http-client-error-contract.mjs";
import { ThingdClient } from "../dist/client.js";

runHttpClientErrorContract("ThingdClient", (url) => new ThingdClient({ url }));
