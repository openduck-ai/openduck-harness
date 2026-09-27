/**
 * Hub Component
 *
 * The empty-chat landing screen. Visually it's "Pair with no messages yet" —
 * a large time + greeting above a centered, narrower ChatInput. Submitting
 * creates a session and navigates to /pair so the rest of the chat lifecycle
 * lives there.
 */

import { useCallback, useEffect, useMemo, useRef, useState } from 'react';
import { defineMessages, useIntl } from '../i18n';
import { AppEvents } from '../constants/events';
import ChatInput from './ChatInput';
import { ChatInputCard } from './ChatInputCard';
import { ChatState } from '../types/chatState';
import 'react-toastify/dist/ReactToastify.css';
import { View, ViewOptions } from '../utils/navigationUtils';
import { useConfig } from './ConfigContext';
import { getInitialWorkingDir } from '../utils/workingDir';
import { createSession } from '../sessions';
import LoadingGoose from './LoadingGoose';
import { UserInput } from '../types/message';
import {
  createNextChatExtensionDraft,
  selectNextChatExtensions,
  type NextChatExtensionDraft,
} from '../utils/nextChatExtensions';
import { formatAcpError } from '../acp/errors';
import { toastError, toastSuccess } from '../toasts';
import { Mail } from 'lucide-react';
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogFooter,
  DialogHeader,
  DialogTitle,
} from './ui/dialog';
import { Input } from './ui/input';
import { Button } from './ui/button';

const i18n = defineMessages({
  goodMorning: { id: 'hub.goodMorning', defaultMessage: 'Good morning' },
  goodAfternoon: { id: 'hub.goodAfternoon', defaultMessage: 'Good afternoon' },
  goodEvening: { id: 'hub.goodEvening', defaultMessage: 'Good evening' },
  projectLabel: { id: 'hub.projectLabel', defaultMessage: 'Project' },
  noRecipients: { id: 'hub.noRecipients', defaultMessage: 'No email recipients configured' },
  setRecipients: { id: 'hub.setRecipients', defaultMessage: 'Set Email Alert' },
  editRecipients: { id: 'hub.editRecipients', defaultMessage: 'Edit Recipients' },
  dialogTitle: { id: 'hub.dialogTitle', defaultMessage: 'Project Email Recipients' },
  dialogDesc: {
    id: 'hub.dialogDesc',
    defaultMessage: 'Configure email recipients for tasks executed under project "{project}":',
  },
  recipientsPlaceholder: {
    id: 'hub.recipientsPlaceholder',
    defaultMessage: 'dev@beiwanai.com, zhanghu@beiwanai.com',
  },
  save: { id: 'hub.save', defaultMessage: 'Save' },
  cancel: { id: 'hub.cancel', defaultMessage: 'Cancel' },
  savedSuccess: {
    id: 'hub.savedSuccess',
    defaultMessage: 'Saved email recipients for {project}',
  },
});

function useClock(): { time: string; meridiem: string; hour: number } {
  const [now, setNow] = useState(() => new Date());
  useEffect(() => {
    const interval = setInterval(() => setNow(new Date()), 30_000);
    return () => clearInterval(interval);
  }, []);

  const hour = now.getHours();
  const minutes = now.getMinutes();
  const meridiem = hour >= 12 ? 'PM' : 'AM';
  const displayHour = ((hour + 11) % 12) + 1;
  const time = `${displayHour}:${String(minutes).padStart(2, '0')}`;
  return { time, meridiem, hour };
}

export default function Hub({
  setView,
}: {
  setView: (view: View, viewOptions?: ViewOptions) => void;
}) {
  const intl = useIntl();
  const { config, extensionsList, upsert } = useConfig();
  const [workingDir, setWorkingDir] = useState(getInitialWorkingDir());
  const [isCreatingSession, setIsCreatingSession] = useState(false);
  const [nextChatExtensionDraft, setNextChatExtensionDraft] =
    useState<NextChatExtensionDraft | null>(null);
  const inputRef = useRef<HTMLTextAreaElement>(null);
  const { time, meridiem, hour } = useClock();

  // Project-based Email Recipients Modal State
  const [isEmailModalOpen, setIsEmailModalOpen] = useState(false);
  const [modalRecipients, setModalRecipients] = useState('');
  const [isSavingRecipients, setIsSavingRecipients] = useState(false);

  const projectSlug = useMemo(() => {
    if (!workingDir) return 'default';
    const parts = workingDir.replace(/\\/g, '/').split('/').filter(Boolean);
    return parts[parts.length - 1] || 'default';
  }, [workingDir]);

  const currentProjectRecipients = useMemo(() => {
    const notif = config?.notifications as Record<string, unknown> | undefined;
    const email = notif?.email as Record<string, unknown> | undefined;
    const projects = (email?.projects || {}) as Record<string, unknown>;
    const val = projects[projectSlug];
    if (Array.isArray(val)) return val.join(', ');
    if (
      val &&
      typeof val === 'object' &&
      Array.isArray((val as Record<string, unknown>).recipients)
    ) {
      return ((val as Record<string, unknown>).recipients as string[]).join(', ');
    }
    return '';
  }, [config, projectSlug]);

  const greeting = useMemo(() => {
    if (hour < 12) return intl.formatMessage(i18n.goodMorning);
    if (hour < 18) return intl.formatMessage(i18n.goodAfternoon);
    return intl.formatMessage(i18n.goodEvening);
  }, [intl, hour]);

  const draftForMenu = useMemo(
    () => nextChatExtensionDraft ?? createNextChatExtensionDraft(extensionsList),
    [extensionsList, nextChatExtensionDraft]
  );

  // rAF is more reliable than autoFocus across async render boundaries.
  useEffect(() => {
    const frameId = requestAnimationFrame(() => {
      inputRef.current?.focus();
    });
    return () => cancelAnimationFrame(frameId);
  }, []);

  const handleNextChatExtensionDraftChange = useCallback((draft: NextChatExtensionDraft) => {
    setNextChatExtensionDraft(draft);
  }, []);

  const handleOpenEmailModal = () => {
    setModalRecipients(currentProjectRecipients);
    setIsEmailModalOpen(true);
  };

  const handleSaveProjectRecipients = async () => {
    setIsSavingRecipients(true);
    try {
      const notif = ((config?.notifications as Record<string, unknown>) || {}) as Record<
        string,
        unknown
      >;
      const email = ((notif.email as Record<string, unknown>) || {
        enabled: true,
        smtp: {},
        default_recipients: [],
        rules: [],
      }) as Record<string, unknown>;
      const projects = { ...((email.projects as Record<string, unknown>) || {}) };

      const recArray = modalRecipients
        .split(',')
        .map((r) => r.trim())
        .filter(Boolean);

      projects[projectSlug] = recArray;

      const updatedNotif = {
        ...notif,
        email: {
          ...email,
          enabled: email.enabled ?? true,
          projects,
        },
      };

      await upsert('notifications', updatedNotif, false);
      toastSuccess({
        title: intl.formatMessage(i18n.dialogTitle),
        msg: intl.formatMessage(i18n.savedSuccess, { project: projectSlug }),
      });
      setIsEmailModalOpen(false);
    } catch (err) {
      console.error('Failed to save project recipients:', err);
      toastError({
        title: intl.formatMessage(i18n.dialogTitle),
        msg: 'Failed to update project email recipients',
      });
    } finally {
      setIsSavingRecipients(false);
    }
  };

  const handleSubmit = async (input: UserInput) => {
    const { msg: userMessage, images } = input;
    if (!(images.length > 0 || userMessage.trim()) || isCreatingSession) return;

    setIsCreatingSession(true);

    try {
      const selectedExtensions = nextChatExtensionDraft
        ? selectNextChatExtensions(extensionsList, nextChatExtensionDraft)
        : [];
      const sessionOptions =
        selectedExtensions.length > 0
          ? { extensionConfigs: selectedExtensions }
          : { allExtensions: extensionsList };

      const session = await createSession(workingDir, sessionOptions);
      setNextChatExtensionDraft(null);

      window.dispatchEvent(new CustomEvent(AppEvents.SESSION_CREATED));
      window.dispatchEvent(
        new CustomEvent(AppEvents.ADD_ACTIVE_SESSION, {
          detail: { sessionId: session.id, initialMessage: { msg: userMessage, images } },
        })
      );

      setView('pair', {
        disableAnimation: true,
        resumeSessionId: session.id,
        initialMessage: { msg: userMessage, images },
      });
    } catch (error) {
      console.error('Failed to create session:', error);
      toastError({ title: "Couldn't start chat", msg: formatAcpError(error) });
      setIsCreatingSession(false);
    }
  };

  return (
    <div className="flex flex-col h-full min-h-0 items-center justify-center px-6 relative">
      <div className="w-full max-w-2xl">
        <div className="flex items-baseline gap-2 mb-1">
          <span className="text-6xl font-light text-text-primary tracking-tight tabular-nums">
            {time}
          </span>
          <span className="text-2xl font-light text-text-secondary">{meridiem}</span>
        </div>
        <p className="text-xl text-text-secondary mb-6">{greeting}</p>

        <ChatInputCard>
          <ChatInput
            sessionId={null}
            handleSubmit={handleSubmit}
            chatState={isCreatingSession ? ChatState.LoadingConversation : ChatState.Idle}
            onStop={() => {}}
            initialValue=""
            setView={setView}
            totalTokens={0}
            accumulatedInputTokens={0}
            accumulatedOutputTokens={0}
            droppedFiles={[]}
            onFilesProcessed={() => {}}
            messages={[]}
            disableAnimation={false}
            onWorkingDirChange={setWorkingDir}
            inputRef={inputRef}
            nextChatExtensionDraft={draftForMenu}
            onNextChatExtensionDraftChange={handleNextChatExtensionDraftChange}
          />
        </ChatInputCard>

        {/* Project-based Email Recipients bar on Hub */}
        <div className="flex items-center justify-between mt-3 px-2 text-xs text-text-secondary">
          <div className="flex items-center gap-1.5 truncate max-w-[75%]">
            <Mail className="h-3.5 w-3.5 text-primary shrink-0" />
            <span>
              {intl.formatMessage(i18n.projectLabel)}:{' '}
              <span className="font-medium text-text-primary">{projectSlug}</span>
            </span>
            <span className="text-gray-400">|</span>
            <span className="truncate">
              {currentProjectRecipients ? (
                <span className="text-text-primary">{currentProjectRecipients}</span>
              ) : (
                <span className="italic text-gray-400">
                  {intl.formatMessage(i18n.noRecipients)}
                </span>
              )}
            </span>
          </div>
          <button
            type="button"
            onClick={handleOpenEmailModal}
            className="text-primary hover:underline hover:text-primary/80 font-medium ml-2 shrink-0 cursor-pointer"
          >
            {currentProjectRecipients
              ? intl.formatMessage(i18n.editRecipients)
              : intl.formatMessage(i18n.setRecipients)}
          </button>
        </div>
      </div>

      {/* Modal for setting Project Email Recipients */}
      <Dialog open={isEmailModalOpen} onOpenChange={setIsEmailModalOpen}>
        <DialogContent className="sm:max-w-md">
          <DialogHeader>
            <DialogTitle className="flex items-center gap-2">
              <Mail className="h-5 w-5 text-primary" />
              {intl.formatMessage(i18n.dialogTitle)}
            </DialogTitle>
            <DialogDescription>
              {intl.formatMessage(i18n.dialogDesc, { project: projectSlug })}
            </DialogDescription>
          </DialogHeader>

          <div className="space-y-3 py-2">
            <Input
              type="text"
              value={modalRecipients}
              onChange={(e) => setModalRecipients(e.target.value)}
              placeholder={intl.formatMessage(i18n.recipientsPlaceholder)}
              autoFocus
            />
            <p className="text-xs text-text-secondary">
              Multiple email addresses can be separated by commas.
            </p>
          </div>

          <DialogFooter className="flex gap-2 justify-end">
            <Button
              type="button"
              variant="ghost"
              onClick={() => setIsEmailModalOpen(false)}
              disabled={isSavingRecipients}
            >
              {intl.formatMessage(i18n.cancel)}
            </Button>
            <Button
              type="button"
              onClick={handleSaveProjectRecipients}
              disabled={isSavingRecipients}
            >
              {isSavingRecipients ? 'Saving...' : intl.formatMessage(i18n.save)}
            </Button>
          </DialogFooter>
        </DialogContent>
      </Dialog>

      {isCreatingSession && (
        <div className="absolute bottom-4 left-4 z-20 pointer-events-none">
          <LoadingGoose chatState={ChatState.LoadingConversation} />
        </div>
      )}
    </div>
  );
}
