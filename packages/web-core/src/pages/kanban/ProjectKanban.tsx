import { useEffect, useMemo, useRef } from 'react';
import { useTranslation } from 'react-i18next';
import { LoginRequiredPrompt } from '@/shared/dialogs/shared/LoginRequiredPrompt';
import { useUserOrganizations } from '@/shared/hooks/useUserOrganizations';
import { useOrganizationProjects } from '@/shared/hooks/useOrganizationProjects';
import { useOrganizationStore } from '@/shared/stores/useOrganizationStore';
import { useAuth } from '@/shared/hooks/auth/useAuth';
import { useCurrentKanbanRouteState } from '@/shared/hooks/useCurrentKanbanRouteState';
import {
  buildKanbanIssueComposerKey,
  closeKanbanIssueComposer,
} from '@/shared/stores/useKanbanIssueComposerStore';
import { useAppNavigation } from '@/shared/hooks/useAppNavigation';

/**
 * Hook to find a project by ID, using orgId from Zustand store
 */
function useFindProjectById(projectId: string | undefined) {
  const { isLoaded: authLoaded } = useAuth();
  const { data: orgsData, isLoading: orgsLoading } = useUserOrganizations();
  const selectedOrgId = useOrganizationStore((s) => s.selectedOrgId);
  const organizations = orgsData?.organizations ?? [];

  // Use stored org ID, or fall back to first org
  const orgIdToUse = selectedOrgId ?? organizations[0]?.id ?? null;

  const { data: projects = [], isLoading: projectsLoading } =
    useOrganizationProjects(orgIdToUse);

  const project = useMemo(() => {
    if (!projectId) return undefined;
    return projects.find((p) => p.id === projectId);
  }, [projectId, projects]);

  return {
    organizationId: project?.organization_id ?? selectedOrgId,
    // Include auth loading state - we can't determine project access until auth loads
    isLoading: !authLoaded || orgsLoading || projectsLoading,
  };
}

/** Redirect legacy project links to the data export page. */
export function ProjectKanban() {
  const { projectId, hostId } = useCurrentKanbanRouteState();
  const { t } = useTranslation('common');
  const { isSignedIn, isLoaded: authLoaded } = useAuth();
  const issueComposerKey = useMemo(() => {
    if (!projectId) {
      return null;
    }
    return buildKanbanIssueComposerKey(hostId, projectId);
  }, [hostId, projectId]);
  const previousIssueComposerKeyRef = useRef<string | null>(null);

  useEffect(() => {
    const previousKey = previousIssueComposerKeyRef.current;
    if (previousKey && previousKey !== issueComposerKey) {
      closeKanbanIssueComposer(previousKey);
    }

    previousIssueComposerKeyRef.current = issueComposerKey;
  }, [issueComposerKey]);

  const { organizationId, isLoading } = useFindProjectById(
    projectId ?? undefined
  );
  const appNavigation = useAppNavigation();
  const canExport = authLoaded && !isLoading && isSignedIn && !!projectId && !!organizationId;
  useEffect(() => {
    if (canExport) appNavigation.goToExport({ replace: true });
  }, [appNavigation, canExport]);

  // Show loading while auth state is being determined
  if (!authLoaded || isLoading) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <p className="text-low">{t('states.loading')}</p>
      </div>
    );
  }

  // If not signed in, prompt user to log in
  if (!isSignedIn) {
    return (
      <div className="flex items-center justify-center h-full w-full p-base">
        <LoginRequiredPrompt
          className="max-w-md"
          title={t('kanban.loginRequired.title')}
          description={t('kanban.loginRequired.description')}
          actionLabel={t('kanban.loginRequired.action')}
        />
      </div>
    );
  }

  if (!projectId || !organizationId) {
    return (
      <div className="flex items-center justify-center h-full w-full">
        <p className="text-low">{t('kanban.noProjectFound')}</p>
      </div>
    );
  }

  return null;
}
