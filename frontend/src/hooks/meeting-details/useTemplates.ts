import { useState, useEffect, useCallback, useRef } from 'react';
import { invoke as invokeTauri } from '@tauri-apps/api/core';
import { listen } from '@tauri-apps/api/event';
import { toast } from 'sonner';
import Analytics from '@/lib/analytics';
import { resolveTemplateSelection } from '@/lib/template-editor';

export interface TemplateInfo { id: string; name: string; description: string }

export function useTemplates() {
  const [availableTemplates, setAvailableTemplates] = useState<TemplateInfo[]>([]);
  const [selectedTemplate, setSelectedTemplate] = useState('standard_meeting');
  const [isLoading, setIsLoading] = useState(true);
  const explicit = useRef(false);
  const selection = useRef('standard_meeting');

  useEffect(() => {
    let disposed = false;
    let request = 0;
    let unlisten: (() => void) | undefined;
    const refresh = async () => {
      const version = ++request;
      try {
        const [templates, defaultId] = await Promise.all([
          invokeTauri<TemplateInfo[]>('api_list_templates'),
          invokeTauri<string>('api_get_default_template'),
        ]);
        if (disposed || request !== version) return;
        setAvailableTemplates(templates);
        const has = (id: string) => templates.some(template => template.id === id);
        const next = resolveTemplateSelection(templates.map(template => template.id), defaultId, selection.current, explicit.current);
        if (!has(selection.current)) explicit.current = false;
        selection.current = next;
        setSelectedTemplate(next);
      } catch (error) {
        if (!disposed && request === version) console.error('Failed to fetch templates:', error);
      } finally {
        if (!disposed && request === version) setIsLoading(false);
      }
    };
    // Wait for native registration to close the initial event delivery gap.
    void listen('templates-updated', () => { void refresh(); }).then(stop => {
      if (disposed) { stop(); return; }
      unlisten = stop;
      void refresh();
    }).catch(error => {
      if (!disposed) { console.error('Failed to watch templates:', error); void refresh(); }
    });
    return () => { disposed = true; request++; unlisten?.(); };
  }, []);

  const handleTemplateSelection = useCallback((templateId: string, templateName: string) => {
    explicit.current = true;
    selection.current = templateId;
    setSelectedTemplate(templateId);
    toast.success('Template selected', { description: `Using "${templateName}" template for summary generation` });
    Analytics.trackFeatureUsed('template_selected');
  }, []);

  return { availableTemplates, selectedTemplate, handleTemplateSelection, isLoading };
}
