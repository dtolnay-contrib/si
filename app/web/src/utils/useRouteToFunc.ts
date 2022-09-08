import { useRoute, useRouter } from "vue-router";

export const useRouteToFunc = () => {
  const route = useRoute();
  const router = useRouter();
  return (funcId?: number) =>
    router.push(`/w/${route.params.workspaceId}/l/${funcId ?? ""}`);
};
