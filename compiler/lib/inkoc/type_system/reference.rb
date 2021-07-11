# frozen_string_literal: true

module Inkoc
  module TypeSystem
    class Reference
      include Type
      include TypeWithPrototype
      include TypeWithAttributes
      include GenericType
      include GenericTypeWithInstances

      extend Forwardable

      attr_reader :type

      def_delegator :type, :prototype
      def_delegator :type, :attributes
      def_delegator :type, :type_parameters
      def_delegator :type, :type_parameter_instances
      def_delegator :type, :type_parameter_instances=
      def_delegator :type, :type_instance?
      def_delegator :type, :type_instance_of?
      def_delegator :type, :generic_type?
      def_delegator :type, :lookup_method
      def_delegator :type, :lookup_attribute
      def_delegator :type, :resolved_return_type
      def_delegator :type, :initialize_type_parameter?
      def_delegator :type, :lookup_type_parameter_instance
      def_delegator :type, :any?
      def_delegator :type, :implements_trait?
      def_delegator :type, :initialize_as
      def_delegator :type, :closure?
      def_delegator :type, :block?
      def_delegator :type, :lambda?
      def_delegator :type, :block_without_signature?
      def_delegator :type, :lookup_type

      def self.wrap(type)
        new(type.is_a?(self) ? type.type : type)
      end

      def initialize(type)
        @type = type
      end

      def reference?
        true
      end

      def as_owned
        type
      end

      def new_instance(param_instances = [])
        self.class.new(type.new_instance(param_instances))
      end

      def base_type
        type.base_type
      end

      def type_name
        "ref #{type.type_name}"
      end

      def type_compatible?(other, state)
        if other.reference?
          type.type_compatible?(other.type, state)
        elsif other.type_parameter?
          type.type_compatible?(other, state)
        elsif other.any?
          true # TODO: `ref T` isn't compatible with `Any`, but is with `ref Any`
        else
          false
        end
      end

      def resolve_self_type(self_type)
        self.class.wrap(type.resolve_self_type(self_type))
      end

      def resolve_type_parameters(self_type, method_type = nil)
        self.class.wrap(type.resolve_type_parameters(self_type, method_type))
      end

      def resolve_type_parameter_with_self(self_type, method_type = nil)
        self.class.wrap(
          type.resolve_type_parameter_with_self(self_type, method_type)
        )
      end

      def remap_using_method_bounds(block_type)
        self.class.wrap(type.remap_using_method_bounds(block_type))
      end

      def with_type_parameter_instances_from(types)
        self.class.wrap(type.with_type_parameter_instances_from(types))
      end

      def with_rigid_type_parameters
        self.class.wrap(type.with_rigid_type_parameters)
      end
    end
  end
end
