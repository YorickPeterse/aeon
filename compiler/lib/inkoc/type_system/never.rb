# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Never
      include Type
      include NewInstance

      def never?
        true
      end

      def type_name
        Config::NEVER_TYPE
      end

      def type_compatible?(*)
        # Never is compatible with everything else because it never returns. This
        # allows one to write code such as:
        #
        #     try { foo } else { vm.panic('oops') }
        true
      end

      def strict_type_compatible?(*)
        true
      end

      def type_instance?
        false
      end

      def new_instance(*)
        self
      end

      def lookup_method(name)
        NullSymbol.singleton
      end
    end
  end
end
