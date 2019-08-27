import { checkAuthentication } from "@/modules/auth";
import {
  OperatingSystemComponent,
  operatingSystemComponentData,
} from "@/datalayer/operating-system-component";
import { GqlRoot, GqlContext, GqlInfo } from "@/app.module";
import {
  GetComponentsInput,
  filterComponents,
} from "@/modules/components/queries";

export async function getOperatingSystemComponents(
  _obj: GqlRoot,
  args: GetComponentsInput,
  _context: GqlContext,
  info: GqlInfo,
): Promise<OperatingSystemComponent[]> {
  const user = await checkAuthentication(info);
  const data: OperatingSystemComponent[] = await operatingSystemComponentData();
  return filterComponents(data, args, user);
}
