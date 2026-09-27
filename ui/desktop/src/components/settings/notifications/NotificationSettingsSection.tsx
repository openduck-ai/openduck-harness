import { useState, useEffect } from 'react';
import { Card, CardContent, CardDescription, CardHeader, CardTitle } from '../../ui/card';
import { Input } from '../../ui/input';
import { Button } from '../../ui/button';
import { useConfig } from '../../ConfigContext';
import { toastSuccess, toastError } from '../../../toasts';
import { Mail, Plus, Trash2, Save } from 'lucide-react';
import { defineMessages, useIntl } from '../../../i18n';

const i18n = defineMessages({
  title: {
    id: 'notificationSettings.title',
    defaultMessage: 'Email Notifications',
  },
  description: {
    id: 'notificationSettings.description',
    defaultMessage: 'Configure task completion email alerts and project-based recipients',
  },
  enableEmail: {
    id: 'notificationSettings.enableEmail',
    defaultMessage: 'Enable Email Notifications',
  },
  smtpConfig: {
    id: 'notificationSettings.smtpConfig',
    defaultMessage: 'SMTP Server Settings',
  },
  host: {
    id: 'notificationSettings.host',
    defaultMessage: 'SMTP Host',
  },
  port: {
    id: 'notificationSettings.port',
    defaultMessage: 'Port',
  },
  useTls: {
    id: 'notificationSettings.useTls',
    defaultMessage: 'Use TLS / SSL',
  },
  username: {
    id: 'notificationSettings.username',
    defaultMessage: 'Username / Email',
  },
  password: {
    id: 'notificationSettings.password',
    defaultMessage: 'Password / Auth Token',
  },
  fromAddress: {
    id: 'notificationSettings.fromAddress',
    defaultMessage: 'From Address (e.g. OpenDuck <bot@example.com>)',
  },
  defaultRecipients: {
    id: 'notificationSettings.defaultRecipients',
    defaultMessage: 'Default Recipients (comma-separated)',
  },
  projectRecipientsTitle: {
    id: 'notificationSettings.projectRecipientsTitle',
    defaultMessage: 'Project-Based Email Recipients',
  },
  projectRecipientsDesc: {
    id: 'notificationSettings.projectRecipientsDesc',
    defaultMessage: 'Route task completion emails to dedicated project teams',
  },
  projectSlug: {
    id: 'notificationSettings.projectSlug',
    defaultMessage: 'Project Slug (e.g. backend_service)',
  },
  recipients: {
    id: 'notificationSettings.recipients',
    defaultMessage: 'Recipients (comma-separated)',
  },
  addProject: {
    id: 'notificationSettings.addProject',
    defaultMessage: 'Add Project Mapping',
  },
  saveChanges: {
    id: 'notificationSettings.saveChanges',
    defaultMessage: 'Save Settings',
  },
  saving: {
    id: 'notificationSettings.saving',
    defaultMessage: 'Saving...',
  },
  savedSuccess: {
    id: 'notificationSettings.savedSuccess',
    defaultMessage: 'Notification settings saved successfully',
  },
  saveFailed: {
    id: 'notificationSettings.saveFailed',
    defaultMessage: 'Failed to save notification settings',
  },
});

interface ProjectRow {
  slug: string;
  recipients: string;
}

export default function NotificationSettingsSection() {
  const intl = useIntl();
  const { config, upsert } = useConfig();

  const [enabled, setEnabled] = useState(false);
  const [host, setHost] = useState('');
  const [port, setPort] = useState(465);
  const [useTls, setUseTls] = useState(true);
  const [username, setUsername] = useState('');
  const [password, setPassword] = useState('');
  const [fromAddress, setFromAddress] = useState('');
  const [defaultRecipients, setDefaultRecipients] = useState('');
  const [projects, setProjects] = useState<ProjectRow[]>([]);
  const [saving, setSaving] = useState(false);

  useEffect(() => {
    const raw = config?.notifications as Record<string, unknown> | undefined;
    const email = raw?.email as Record<string, unknown> | undefined;

    if (email) {
      setEnabled(Boolean(email.enabled));
      const smtp = email.smtp as Record<string, unknown> | undefined;
      if (smtp) {
        setHost(String(smtp.host || ''));
        setPort(Number(smtp.port || 465));
        setUseTls(smtp.use_tls !== false);
        setUsername(String(smtp.username || ''));
        setPassword(String(smtp.password || ''));
        setFromAddress(String(smtp.from || ''));
      }
      const defRec = (email.default_recipients as string[]) || [];
      setDefaultRecipients(defRec.join(', '));

      const projMap = (email.projects as Record<string, unknown>) || {};
      const projRows: ProjectRow[] = [];
      for (const [slug, val] of Object.entries(projMap)) {
        if (Array.isArray(val)) {
          projRows.push({ slug, recipients: val.join(', ') });
        } else if (val && typeof val === 'object' && Array.isArray((val as Record<string, unknown>).recipients)) {
          projRows.push({
            slug,
            recipients: ((val as Record<string, unknown>).recipients as string[]).join(', '),
          });
        }
      }
      setProjects(projRows);
    }
  }, [config]);

  const handleAddProject = () => {
    setProjects([...projects, { slug: '', recipients: '' }]);
  };

  const handleRemoveProject = (index: number) => {
    setProjects(projects.filter((_, i) => i !== index));
  };

  const handleProjectChange = (index: number, field: keyof ProjectRow, value: string) => {
    const updated = [...projects];
    updated[index][field] = value;
    setProjects(updated);
  };

  const handleSave = async () => {
    setSaving(true);
    try {
      const projectsMap: Record<string, string[]> = {};
      for (const row of projects) {
        if (row.slug.trim()) {
          const recs = row.recipients
            .split(',')
            .map((r) => r.trim())
            .filter(Boolean);
          projectsMap[row.slug.trim()] = recs;
        }
      }

      const defRecs = defaultRecipients
        .split(',')
        .map((r) => r.trim())
        .filter(Boolean);

      const notificationPayload = {
        email: {
          enabled,
          smtp: {
            host: host.trim(),
            port: Number(port),
            use_tls: useTls,
            username: username.trim() || undefined,
            password: password.trim() || undefined,
            from: fromAddress.trim() || 'OpenDuck <noreply@localhost>',
          },
          default_recipients: defRecs,
          projects: projectsMap,
          rules: [
            {
              trigger_on: 'on_failure',
              recipients: defRecs,
            },
          ],
        },
      };

      await upsert('notifications', notificationPayload, false);
      toastSuccess({
        title: intl.formatMessage(i18n.title),
        msg: intl.formatMessage(i18n.savedSuccess),
      });
    } catch {
      toastError({
        title: intl.formatMessage(i18n.title),
        msg: intl.formatMessage(i18n.saveFailed),
      });
    } finally {
      setSaving(false);
    }
  };

  return (
    <div className="space-y-6 pb-12">
      <Card>
        <CardHeader>
          <CardTitle className="flex items-center gap-2">
            <Mail className="h-5 w-5 text-primary" />
            {intl.formatMessage(i18n.title)}
          </CardTitle>
          <CardDescription>{intl.formatMessage(i18n.description)}</CardDescription>
        </CardHeader>
        <CardContent className="space-y-4">
          <label className="flex items-center gap-2 cursor-pointer font-medium text-sm">
            <input
              type="checkbox"
              checked={enabled}
              onChange={(e) => setEnabled(e.target.checked)}
              className="rounded text-primary focus:ring-primary h-4 w-4"
            />
            {intl.formatMessage(i18n.enableEmail)}
          </label>

          {enabled && (
            <div className="space-y-4 pt-2 border-t border-border-primary">
              <h4 className="text-sm font-semibold">{intl.formatMessage(i18n.smtpConfig)}</h4>
              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-text-secondary mb-1">
                    {intl.formatMessage(i18n.host)}
                  </label>
                  <Input
                    type="text"
                    value={host}
                    onChange={(e) => setHost(e.target.value)}
                    placeholder="smtp.feishu.cn"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-text-secondary mb-1">
                    {intl.formatMessage(i18n.port)}
                  </label>
                  <Input
                    type="number"
                    value={port}
                    onChange={(e) => setPort(Number(e.target.value))}
                    placeholder="465"
                  />
                </div>
              </div>

              <div className="grid grid-cols-2 gap-4">
                <div>
                  <label className="block text-xs font-medium text-text-secondary mb-1">
                    {intl.formatMessage(i18n.username)}
                  </label>
                  <Input
                    type="text"
                    value={username}
                    onChange={(e) => setUsername(e.target.value)}
                    placeholder="cc@beiwanai.com"
                  />
                </div>
                <div>
                  <label className="block text-xs font-medium text-text-secondary mb-1">
                    {intl.formatMessage(i18n.password)}
                  </label>
                  <Input
                    type="password"
                    value={password}
                    onChange={(e) => setPassword(e.target.value)}
                    placeholder="••••••••"
                  />
                </div>
              </div>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  {intl.formatMessage(i18n.fromAddress)}
                </label>
                <Input
                  type="text"
                  value={fromAddress}
                  onChange={(e) => setFromAddress(e.target.value)}
                  placeholder="OpenDuck <cc@beiwanai.com>"
                />
              </div>

              <div>
                <label className="block text-xs font-medium text-text-secondary mb-1">
                  {intl.formatMessage(i18n.defaultRecipients)}
                </label>
                <Input
                  type="text"
                  value={defaultRecipients}
                  onChange={(e) => setDefaultRecipients(e.target.value)}
                  placeholder="zhanghu@beiwanai.com, team@beiwanai.com"
                />
              </div>
            </div>
          )}
        </CardContent>
      </Card>

      {enabled && (
        <Card>
          <CardHeader>
            <CardTitle>{intl.formatMessage(i18n.projectRecipientsTitle)}</CardTitle>
            <CardDescription>{intl.formatMessage(i18n.projectRecipientsDesc)}</CardDescription>
          </CardHeader>
          <CardContent className="space-y-3">
            {projects.map((proj, idx) => (
              <div key={idx} className="flex gap-2 items-center">
                <div className="w-1/3">
                  <Input
                    type="text"
                    value={proj.slug}
                    onChange={(e) => handleProjectChange(idx, 'slug', e.target.value)}
                    placeholder={intl.formatMessage(i18n.projectSlug)}
                  />
                </div>
                <div className="flex-1">
                  <Input
                    type="text"
                    value={proj.recipients}
                    onChange={(e) => handleProjectChange(idx, 'recipients', e.target.value)}
                    placeholder={intl.formatMessage(i18n.recipients)}
                  />
                </div>
                <Button
                  type="button"
                  variant="ghost"
                  size="sm"
                  onClick={() => handleRemoveProject(idx)}
                  className="text-text-danger hover:bg-red-50 dark:hover:bg-red-950/20"
                >
                  <Trash2 className="h-4 w-4" />
                </Button>
              </div>
            ))}

            <Button
              type="button"
              variant="outline"
              onClick={handleAddProject}
              className="flex items-center gap-1.5 mt-2"
            >
              <Plus className="h-4 w-4" />
              {intl.formatMessage(i18n.addProject)}
            </Button>
          </CardContent>
        </Card>
      )}

      <div className="flex justify-end gap-3">
        <Button onClick={handleSave} disabled={saving} className="flex items-center gap-2">
          <Save className="h-4 w-4" />
          {saving ? intl.formatMessage(i18n.saving) : intl.formatMessage(i18n.saveChanges)}
        </Button>
      </div>
    </div>
  );
}
