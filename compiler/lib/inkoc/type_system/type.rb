# frozen_string_literal: true

module Inkoc
  module TypeSystem
    # Module defining various (required) methods for regular types.
    # rubocop: disable Metrics/ModuleLength
    module Type
      def type_name
        raise NotImplementedError, "#{self.class} does not implement #type_name"
      end

      def owned?
        !reference?
      end

      def reference?
        false
      end

      def block?
        false
      end

      def method?
        false
      end

      def closure?
        false
      end

      def lambda?
        false
      end

      def object?
        false
      end

      def any?
        false
      end

      def trait?
        false
      end

      def error?
        false
      end

      def never?
        false
      end

      def generic_type?
        false
      end

      def generic_object?
        false
      end

      def type_parameter?
        false
      end

      def rigid_type_parameter?
        false
      end

      def self_type?
        false
      end

      def new_instance(_param_instances = [])
        self
      end

      def implements_trait?(*)
        false
      end

      def type_compatible?(_other, _state)
        false
      end

      def strict_type_compatible?(other, state)
        return false if owned? && other.reference?
        return false if reference? && other.owned? && !other.type_parameter?

        type_compatible?(other, state)
      end

      # Returns true if `self` is compatible with the given type parameter.
      def compatible_with_type_parameter?(param, state)
        param.required_traits.all? do |trait|
          type_compatible?(trait, state)
        end
      end

      def cast_to?(cast_to, state)
        if reference? && cast_to.reference?
          from = type
          to = cast_to.type
        elsif owned? && cast_to.owned?
          from = self
          to = cast_to
        else
          return false
        end

        to.type_compatible?(from, state) || from.type_compatible?(to, state)
      end

      def resolve_self_type(_self_type)
        self
      end

      def downcast_to(_other)
        self
      end

      def initialize_as(_type, _method_type, _self_type)
        nil
      end

      def implementation_of?(_block, _state)
        false
      end

      def remap_using_method_bounds(_block_type)
        self
      end

      def without_empty_type_parameters(_self_type, _block_type)
        self
      end

      def initialize_type_parameter?(_param)
        false
      end

      def resolve_type_parameters(_self_type, _method_type = nil)
        self
      end

      def resolve_type_parameter_with_self(_self_type, _method_type = nil)
        self
      end

      def with_rigid_type_parameters
        self
      end

      def as_owned
        self
      end

      def as_reference_or_owned(owned:)
        owned ? self : Reference.wrap(self)
      end

      def responds_to_truthy?
        lookup_method(Config::TRUTHY_MESSAGE).any?
      end
    end
    # rubocop: enable Metrics/ModuleLength
  end
end
