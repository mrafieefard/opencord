import { AddServerForm } from '@/features/servers/AddServerForm';
import { SheetToasts } from '@/ui';

export default function AddServerScreen() {
  return (
    <>
      <AddServerForm />
      <SheetToasts />
    </>
  );
}
