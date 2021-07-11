# frozen_string_literal: true

module Inkoc
  module TypeSystem
    module GenericTypeWithInstances
      def type_parameter_instances
        raise NotImplementedError
      end

      def lookup_type_parameter_instance(param)
        type_parameter_instances[param]
      end

      def initialize_type_parameter(param, value)
        type_parameter_instances.define(param, value)
      end

      def initialize_type_parameters_in_order(instances)
        type_parameters.zip(instances) do |param, instance|
          initialize_type_parameter(param, instance) if instance
        end
      end

      def initialize_type_parameter?(param)
        if defines_type_parameter?(param)
          instance = type_parameter_instances[param]

          instance.nil? || instance.type_parameter?
        else
          false
        end
      end

      # Returns true if our type is compatible with the given generic type.
      def compatible_with_generic_type?(other, state)
        # This ensures we're not comparing an A!(X) with a B!(X), which are two
        # completely different types. This automatically takes into account the
        # number of type parameters as these are defined on the base type (and
        # thus the same after this check).
        return false if base_type != other.base_type

        # If we share the same base type, but we are not initialised yet, then
        # we are compatible with "other". One such example would be comparing an
        # empty Array with a type `Array!(Foo)`.
        return true if type_parameter_instances.empty?

        type_parameters.all? do |param|
          ours = lookup_type_parameter_instance(param) || param
          theirs = other.lookup_type_parameter_instance(param) || param

          ours == param || ours.type_compatible?(theirs, state)
        end
      end

      def resolve_type_parameters(self_type, method_type = nil)
        rtype = resolve_self_type(self_type)

        if rtype.generic_type?
          new_instances = rtype.type_parameter_instances.dup

          # Copy over any type parameter instances the new type may use.
          rtype.type_parameters.each do |param|
            direct_instance = rtype.lookup_type_parameter_instance(param)

            break unless direct_instance

            # If the initialised value is a type parameter we need to look up
            # its actual instance in either the block or the current type of
            # "self".
            instance =
              if direct_instance&.type_parameter?
                method_type&.lookup_type_parameter_instance(direct_instance) ||
                  self_type.lookup_type_parameter_instance(direct_instance)
              else
                direct_instance.resolve_type_parameters(self_type, method_type)
              end

            new_instances.define(param, instance) if instance
          end

          # We need a copy of the return type so we don't modify it in-place, as
          # doing so could mess up future use of this method.
          rtype = rtype.new_instance.tap do |copy|
            copy.type_parameter_instances = new_instances
          end
        end

        rtype.resolve_type_parameter_with_self(self_type, method_type)
      end

      def with_rigid_type_parameters
        return self unless generic_type?

        dup.tap do |copy|
          copy.type_parameter_instances =
            type_parameter_instances.with_rigid_type_parameters
        end
      end
    end
  end
end
