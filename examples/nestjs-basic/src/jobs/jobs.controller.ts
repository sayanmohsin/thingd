import { BadRequestException, Body, Controller, Get, Post } from "@nestjs/common";
// biome-ignore lint/style/useImportType: NestJS constructor injection needs runtime metadata.
import { MemorydService } from "../memoryd/memoryd.service";

type CreateEmbedJobBody = {
  object?: string;
};

@Controller("jobs")
export class JobsController {
  constructor(private readonly memoryd: MemorydService) {}

  @Post("embed")
  enqueueEmbedJob(@Body() body: CreateEmbedJobBody) {
    if (!body.object) {
      throw new BadRequestException("object is required");
    }

    return this.memoryd.pushJob("embed", {
      object: body.object,
    });
  }

  @Get("embed")
  listEmbedJobs() {
    return this.memoryd.listJobs("embed");
  }
}
