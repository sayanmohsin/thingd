import { BadRequestException, Body, Controller, Get, Post } from "@nestjs/common";
// biome-ignore lint/style/useImportType: NestJS constructor injection needs runtime metadata.
import { ThingdService } from "../thingd/thingd.service";

type CreateEmbedJobBody = {
  object?: string;
};

@Controller("jobs")
export class JobsController {
  constructor(private readonly thingd: ThingdService) {}

  @Post("embed")
  enqueueEmbedJob(@Body() body: CreateEmbedJobBody) {
    if (!body.object) {
      throw new BadRequestException("object is required");
    }

    return this.thingd.pushJob("embed", {
      object: body.object,
    });
  }

  @Get("embed")
  listEmbedJobs() {
    return this.thingd.listJobs("embed");
  }
}
