import {
  Show,
  createContext,
  createEffect,
  createSignal,
  createUniqueId,
  onCleanup,
  splitProps,
  useContext,
} from 'solid-js';
import type { JSX } from 'solid-js';

interface DialogOpenChangeDetails {
  open: boolean;
}

interface DialogContextValue {
  closeOnEscape: boolean;
  closeOnInteractOutside: boolean;
  descriptionId: () => string | undefined;
  open: () => boolean;
  setOpen: (open: boolean) => void;
  setDescriptionId: (id: string | undefined) => void;
  setTitleId: (id: string | undefined) => void;
  titleId: () => string | undefined;
}

interface DialogRootProps {
  children: JSX.Element;
  closeOnEscape?: boolean;
  closeOnInteractOutside?: boolean;
  lazyMount?: boolean;
  onEscapeKeyDown?: (event: KeyboardEvent) => void;
  onInteractOutside?: (event: PointerEvent) => void;
  onOpenChange?: (details: DialogOpenChangeDetails) => void;
  open: boolean;
  role?: 'dialog' | 'alertdialog';
  unmountOnExit?: boolean;
}

interface DialogRenderTriggerProps extends JSX.ButtonHTMLAttributes<HTMLButtonElement> {
  'aria-expanded': boolean;
  'aria-haspopup': 'dialog';
  'data-state': 'open' | 'closed';
}

interface DialogTriggerProps extends Omit<JSX.ButtonHTMLAttributes<HTMLButtonElement>, 'onClick'> {
  asChild?: (props: () => DialogRenderTriggerProps) => JSX.Element;
  onClick?: (event: MouseEvent) => void;
}

interface DialogCloseTriggerProps extends Omit<
  JSX.ButtonHTMLAttributes<HTMLButtonElement>,
  'onClick'
> {
  asChild?: (props: () => JSX.ButtonHTMLAttributes<HTMLButtonElement>) => JSX.Element;
  onClick?: (event: MouseEvent) => void;
}

interface DialogBackdropProps extends Omit<
  JSX.ButtonHTMLAttributes<HTMLButtonElement>,
  'onClick' | 'onPointerDown' | 'type'
> {
  onClick?: (event: MouseEvent) => void;
  onPointerDown?: (event: PointerEvent) => void;
}

interface DialogContentProps extends JSX.HTMLAttributes<HTMLDivElement> {
  ref?: (element: HTMLDivElement) => void;
}

const DialogContext = createContext<DialogContextValue>();

function useDialogContext() {
  const context = useContext(DialogContext);
  if (!context) {
    throw new Error('Dialog primitives must be rendered inside <Dialog.Root>');
  }
  return context;
}

function Root(props: DialogRootProps) {
  const [titleId, setTitleId] = createSignal<string>();
  const [descriptionId, setDescriptionId] = createSignal<string>();
  const setOpen = (open: boolean) => {
    if (open !== props.open) {
      props.onOpenChange?.({ open });
    }
  };

  const context: DialogContextValue = {
    closeOnEscape: props.closeOnEscape ?? true,
    closeOnInteractOutside: props.closeOnInteractOutside ?? true,
    descriptionId,
    open: () => props.open,
    setOpen,
    setDescriptionId,
    setTitleId,
    titleId,
  };

  createEffect(() => {
    if (!props.open) {
      return;
    }

    const handleKeyDown = (event: KeyboardEvent) => {
      if (event.key !== 'Escape') {
        return;
      }
      props.onEscapeKeyDown?.(event);
      if (context.closeOnEscape && !event.defaultPrevented) {
        setOpen(false);
      }
    };

    document.addEventListener('keydown', handleKeyDown);
    onCleanup(() => document.removeEventListener('keydown', handleKeyDown));
  });

  return <DialogContext.Provider value={context}>{props.children}</DialogContext.Provider>;
}

function Trigger(props: DialogTriggerProps) {
  const context = useDialogContext();
  const [, rest] = splitProps(props, ['asChild', 'onClick', 'children']);
  const triggerProps = (): DialogRenderTriggerProps => ({
    ...rest,
    'aria-expanded': context.open(),
    'aria-haspopup': 'dialog',
    'data-state': context.open() ? 'open' : 'closed',
    onClick: (event) => {
      props.onClick?.(event);
      if (!event.defaultPrevented) {
        context.setOpen(true);
      }
    },
  });

  if (props.asChild) {
    return <>{props.asChild(triggerProps)}</>;
  }

  return (
    <button type="button" {...triggerProps()}>
      {props.children}
    </button>
  );
}

function Backdrop(props: DialogBackdropProps) {
  const context = useDialogContext();
  const [, rest] = splitProps(props, ['onClick', 'onPointerDown']);

  return (
    <Show when={context.open()}>
      <button
        {...rest}
        type="button"
        data-dialog-backdrop
        data-state="open"
        onPointerDown={(event) => {
          props.onPointerDown?.(event);
          if (context.closeOnInteractOutside && !event.defaultPrevented) {
            context.setOpen(false);
          }
        }}
        onClick={(event) => {
          props.onClick?.(event);
          if (context.closeOnInteractOutside && !event.defaultPrevented) {
            context.setOpen(false);
          }
        }}
      />
    </Show>
  );
}

function Positioner(props: JSX.HTMLAttributes<HTMLDivElement>) {
  const context = useDialogContext();
  return (
    <Show when={context.open()}>
      <div {...props} data-state="open">
        {props.children}
      </div>
    </Show>
  );
}

function Content(props: DialogContentProps) {
  const context = useDialogContext();
  let contentRef: HTMLDivElement | undefined;
  const [, rest] = splitProps(props, ['children', 'ref']);

  createEffect(() => {
    if (context.open()) {
      queueMicrotask(() => contentRef?.focus());
    }
  });

  return (
    <Show when={context.open()}>
      <div
        {...rest}
        ref={(element) => {
          contentRef = element;
          props.ref?.(element);
        }}
        role="dialog"
        aria-modal="true"
        aria-labelledby={context.titleId()}
        aria-describedby={context.descriptionId()}
        tabIndex={-1}
        data-state="open"
      >
        {props.children}
      </div>
    </Show>
  );
}

function Title(props: JSX.HTMLAttributes<HTMLHeadingElement>) {
  const context = useDialogContext();
  const id = props.id ?? createUniqueId();
  createEffect(() => {
    context.setTitleId(id);
    onCleanup(() => context.setTitleId(undefined));
  });

  return (
    <h2 {...props} id={id}>
      {props.children}
    </h2>
  );
}

function Description(props: JSX.HTMLAttributes<HTMLParagraphElement>) {
  const context = useDialogContext();
  const id = props.id ?? createUniqueId();
  createEffect(() => {
    context.setDescriptionId(id);
    onCleanup(() => context.setDescriptionId(undefined));
  });

  return (
    <p {...props} id={id}>
      {props.children}
    </p>
  );
}

function CloseTrigger(props: DialogCloseTriggerProps) {
  const context = useDialogContext();
  const [, rest] = splitProps(props, ['asChild', 'onClick', 'children']);
  const closeProps = (): JSX.ButtonHTMLAttributes<HTMLButtonElement> => ({
    ...rest,
    onClick: (event) => {
      props.onClick?.(event);
      if (!event.defaultPrevented) {
        context.setOpen(false);
      }
    },
  });

  if (props.asChild) {
    return <>{props.asChild(closeProps)}</>;
  }

  return (
    <button type="button" {...closeProps()}>
      {props.children}
    </button>
  );
}

export const Dialog = {
  Backdrop,
  CloseTrigger,
  Content,
  Description,
  Positioner,
  Root,
  Title,
  Trigger,
};
