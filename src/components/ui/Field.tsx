import {
  createContext,
  createEffect,
  createSignal,
  createUniqueId,
  onCleanup,
  splitProps,
  useContext,
} from 'solid-js';
import type { JSX } from 'solid-js';

interface FieldContextValue {
  controlId: () => string;
  describedBy: () => string | undefined;
  disabled: () => boolean;
  errorId: () => string | undefined;
  helperId: () => string | undefined;
  invalid: () => boolean;
  setErrorId: (id: string | undefined) => void;
  setHelperId: (id: string | undefined) => void;
}

interface FieldRootProps extends JSX.HTMLAttributes<HTMLDivElement> {
  disabled?: boolean;
  invalid?: boolean;
}

type FieldLabelProps = JSX.LabelHTMLAttributes<HTMLLabelElement>;

interface FieldInputRenderProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  'aria-describedby'?: string;
  'aria-invalid': boolean | undefined;
}

interface FieldInputProps extends JSX.InputHTMLAttributes<HTMLInputElement> {
  asChild?: (props: () => FieldInputRenderProps) => JSX.Element;
}

interface FieldTextareaRenderProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  'aria-describedby'?: string;
  'aria-invalid': boolean | undefined;
}

interface FieldTextareaProps extends JSX.TextareaHTMLAttributes<HTMLTextAreaElement> {
  asChild?: (props: () => FieldTextareaRenderProps) => JSX.Element;
}

const FieldContext = createContext<FieldContextValue>();

function useFieldContext() {
  const context = useContext(FieldContext);
  if (!context) {
    throw new Error('Field primitives must be rendered inside <Field.Root>');
  }
  return context;
}

function Root(props: FieldRootProps) {
  const controlId = createUniqueId();
  const [helperId, setHelperId] = createSignal<string>();
  const [errorId, setErrorId] = createSignal<string>();
  const [, rest] = splitProps(props, ['children', 'disabled', 'invalid']);
  const describedBy = () => [helperId(), errorId()].filter(Boolean).join(' ') || undefined;
  const context: FieldContextValue = {
    controlId: () => controlId,
    describedBy,
    disabled: () => props.disabled ?? false,
    errorId,
    helperId,
    invalid: () => props.invalid ?? false,
    setErrorId,
    setHelperId,
  };

  return (
    <FieldContext.Provider value={context}>
      <div
        {...rest}
        data-disabled={context.disabled() ? '' : undefined}
        data-invalid={context.invalid() ? '' : undefined}
      >
        {props.children}
      </div>
    </FieldContext.Provider>
  );
}

function Label(props: FieldLabelProps) {
  const context = useFieldContext();
  return (
    <label {...props} for={props.for ?? context.controlId()}>
      {props.children}
    </label>
  );
}

function inputRenderProps(
  context: FieldContextValue,
  props: JSX.InputHTMLAttributes<HTMLInputElement>,
): FieldInputRenderProps {
  return {
    ...props,
    'aria-describedby': context.describedBy(),
    'aria-invalid': context.invalid() ? true : undefined,
    disabled: props.disabled ?? context.disabled(),
    id: props.id ?? context.controlId(),
  };
}

function Input(props: FieldInputProps) {
  const context = useFieldContext();
  const [, rest] = splitProps(props, ['asChild']);
  const fieldProps = () => inputRenderProps(context, rest);

  if (props.asChild) {
    return <>{props.asChild(fieldProps)}</>;
  }

  return <input {...fieldProps()} />;
}

function textareaRenderProps(
  context: FieldContextValue,
  props: JSX.TextareaHTMLAttributes<HTMLTextAreaElement>,
): FieldTextareaRenderProps {
  return {
    ...props,
    'aria-describedby': context.describedBy(),
    'aria-invalid': context.invalid() ? true : undefined,
    disabled: props.disabled ?? context.disabled(),
    id: props.id ?? context.controlId(),
  };
}

function Textarea(props: FieldTextareaProps) {
  const context = useFieldContext();
  const [, rest] = splitProps(props, ['asChild']);
  const fieldProps = () => textareaRenderProps(context, rest);

  if (props.asChild) {
    return <>{props.asChild(fieldProps)}</>;
  }

  return <textarea {...fieldProps()} />;
}

function ErrorText(props: JSX.HTMLAttributes<HTMLParagraphElement>) {
  const context = useFieldContext();
  const id = props.id ?? createUniqueId();
  createEffect(() => {
    context.setErrorId(id);
    onCleanup(() => context.setErrorId(undefined));
  });

  return (
    <p {...props} id={id}>
      {props.children}
    </p>
  );
}

function HelperText(props: JSX.HTMLAttributes<HTMLParagraphElement>) {
  const context = useFieldContext();
  const id = props.id ?? createUniqueId();
  createEffect(() => {
    context.setHelperId(id);
    onCleanup(() => context.setHelperId(undefined));
  });

  return (
    <p {...props} id={id}>
      {props.children}
    </p>
  );
}

export const Field = {
  ErrorText,
  HelperText,
  Input,
  Label,
  Root,
  Textarea,
};
